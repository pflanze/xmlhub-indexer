use std::{
    env::current_dir,
    ffi::OsString,
    fmt::Debug,
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;

use debug_ignore::DebugIgnore;
use xmlhub_indexer::{
    cargo::{
        check_cargo_toml_no_path, run_cargo, CompilationProfile, CompilationTarget, Env,
        TargetTriple,
    },
    changelog::CHANGELOG_FILE_NAME,
    checkout_context::CheckExpectedSubpathsExist,
    effect::{bind, Effect, NoOp},
    git::GitWorkingDir,
    git_version::{GitVersion, SemVersion},
    installation::{
        app_info::AppInfo,
        binaries_repo::{Arch, Os},
        copy_file::copy_file,
    },
    installation::{
        app_signature::{AppSignaturePrivateKey, SaveLoadKeyFile},
        binaries_repo::BinariesRepoSection,
        json_file::JsonFile,
        util::{get_creator, get_timestamp},
    },
    path_util::AppendToPath,
    sha256::sha256sum_paranoid,
    util::{ask_yn, create_dir_levels_if_necessary, hostname, prog_version, stringify_error},
    xmlhub_indexer_defaults::{BINARIES_CHECKOUT, SOURCE_CHECKOUT, XMLHUB_BINARY_FILE_NAME},
};

#[derive(clap::Parser, Debug)]
#[clap(next_line_help = true)]
/// Produce a new release of the xmlhub-indexer repository, both the
/// source code as well as a `xmlhub` binary if on macOS
/// or Linux. Also creates a new version tag if necessary.
struct Opts {
    /// By default, the program shows the actions to be carried out
    /// and then asks for confirmation. This option ends the program
    /// without asking for confirmation or carrying them out.
    #[clap(long)]
    dry_run: bool,

    /// Show the actions to be carried out in debug format instead of
    /// a bullet list.
    #[clap(long)]
    verbose: bool,

    /// Carry out the actions without asking for confirmation
    /// (basically the opposite of --dry-run).
    #[clap(long)]
    yes: bool,

    /// Use this option if you're confident that the files written by
    /// `xmlhub` are not created differently due to changes in
    /// the program (note: even formatting changes matter: the aim of
    /// the versioning is to prevent a situation where different
    /// people each run different versions of the xmlhub-indexer and
    /// both commit the changes over each other endlessly without any
    /// XML files having changed). Increases only the minor part of the
    /// version number (e.g. `v2` becomes `v2.1` instead of `v3`, and
    /// `v2.1` becomes `v2.2` instead of `v3`).
    #[clap(long)]
    unchanged_output: bool,

    /// Do not publish the built binary to the
    /// `xmlhub-indexer-binaries` repository. By default this is done
    /// for macOS. This option disables that.
    #[clap(long)]
    no_publish_binary: bool,

    /// Sign the Git tags with your PGP key, via the git tag -s
    /// option. This is the default. Note that you probably also need
    /// to give the `--local-user` option.
    #[clap(long)]
    sign: bool,

    /// Do not sign the Git tags. Use this only if you have decided
    /// not to sign releases and binaries. The default is to sign.
    #[clap(long)]
    no_sign: bool,

    /// When signing, the name of the key to use. This is passed on to
    /// `git tag`, which passes it to `gpg`. The key fingerprint works
    /// best. If not given, signing assumes the default key name,
    /// which may fail; you now need to give the
    /// `--no-require-local-user` option to allow that.
    #[clap(long)]
    local_user: Option<String>,

    /// When releasing a binary, the path to an app signature private
    /// key is required. Generate such a key pair via
    /// `xmlhub-indexer-signature gen-key`. But the key also has to be
    /// part of XXX trust list or it won't be accepted when XXX
    /// upgrading.
    #[clap(long)]
    app_private_key: Option<PathBuf>,

    /// In my experience on the Mac, the `--local-user` option is
    /// needed when signing; if you want to try it anyway, omit the
    /// `--local-user` option and give this option instead, then tell
    /// me to change the program if it works.
    #[clap(long)]
    no_require_local_user: bool,

    /// Push the branch and Git tag in the *source* repository to the
    /// default remote (presumed to be the upstream repository), for
    /// both the source and binary repositories. This is the default.
    #[clap(long)]
    push_source: bool,

    /// Do not push the branch and Git tag to the *source*
    /// repository. The default is to push.
    #[clap(long)]
    no_push_source: bool,

    /// Push the branch and Git tag in the *binary* repository to the
    /// default remote (presumed to be the upstream repository), for
    /// both the source and binary repositories. This is the default.
    #[clap(long)]
    push_binaries: bool,

    /// Do not push the branch and Git tag to the *binary*
    /// repository. The default is to push.
    #[clap(long)]
    no_push_binaries: bool,
}

#[derive(Debug)]
struct ChangelogTag {
    tag_name: String,
    date: String,
}

impl ChangelogTag {
    fn release_line(&self) -> String {
        let ChangelogTag { tag_name, date } = self;
        format!("{tag_name} - {date}")
    }

    fn commit_message(&self) -> String {
        format!("Update Changelog for release {}", self.tag_name)
    }
}

#[derive(Debug)]
struct UpdateChangelogWithTag {
    changelog_path: PathBuf,
    changelog_tag: ChangelogTag,
}

#[derive(Debug)]
struct UpdatedChangelog {
    source_commit_id: String,
    tag_name: String,
}

impl Effect for UpdateChangelogWithTag {
    type Requires = ();

    type Provides = UpdatedChangelog;

    fn show_bullet_points(&self) -> String {
        let Self {
            changelog_path,
            changelog_tag,
        } = self;
        format!(
            "  * add the line {:?} to {changelog_path:?} and commit",
            changelog_tag.release_line()
        )
    }

    fn run(self: Box<Self>, (): Self::Requires) -> Result<Self::Provides> {
        let UpdateChangelogWithTag {
            changelog_path,
            changelog_tag,
        } = *self;

        // "v8 - 2025-04-11"
        let mut out = OpenOptions::new()
            .create(false)
            .append(true)
            .open(&changelog_path)
            .with_context(|| {
                anyhow!("opening {changelog_path:?} for appending, expecting it to exist")
            })?;
        let release_line = changelog_tag.release_line();
        write!(&mut out, "\n{release_line}\n\n")
            .with_context(|| anyhow!("writing to {changelog_path:?}"))?;
        out.flush()?; // unbuffered anyway? error with context?

        let source_checkout = &SOURCE_CHECKOUT;

        // Now also commit it. (Ah, passing around repositories in
        // globals, simply.)
        if !source_checkout.git_working_dir().git(
            &[
                OsString::from("commit"),
                OsString::from("-m"),
                OsString::from(changelog_tag.commit_message()),
                // Git is OK with absolute paths
                changelog_path.canonicalize()?.into(),
            ],
            false,
        )? {
            bail!("git commit for {changelog_path:?} returned false");
        }

        let source_commit_id = source_checkout.git_working_dir().get_head_commit_id()?;

        Ok(UpdatedChangelog {
            source_commit_id,
            tag_name: changelog_tag.tag_name,
        })
    }
}

// Don't need to store tag_name String in here since it's constant and
// part of the individual contexts that need it.
#[derive(Debug)]
struct SourceReleaseTag {
    source_commit_id: String,
}

/// Tag name is taken from UpdatedChangelog
#[derive(Debug)]
struct CreateTag {
    sign: bool,
    local_user: Option<String>,
}

impl Effect for CreateTag {
    type Requires = UpdatedChangelog;
    type Provides = SourceReleaseTag;

    fn show_bullet_points(&self) -> String {
        let Self { sign, local_user } = self;
        if *sign {
            format!(
                "  * create a tag for the given version, signed with key {}",
                local_user
                    .as_ref()
                    .expect("user is now required when signing")
            )
        } else {
            format!("  * create an unsigned tag for the given version")
        }
    }

    fn run(self: Box<Self>, provided: Self::Requires) -> Result<Self::Provides> {
        let UpdatedChangelog {
            source_commit_id,
            tag_name,
        } = provided;
        SOURCE_CHECKOUT.git_working_dir().git_tag(
            &tag_name,
            None,
            "via make-release.rs",
            self.sign,
            self.local_user.as_deref(),
        )?;

        // Make sure cargo will rebuild the binary (see build.rs)
        {
            let path_str = ".released_version";
            let path = PathBuf::from(path_str);
            std::fs::write(&path, &tag_name)
                .with_context(|| anyhow!("path should be writable: {path:?}"))?;
            // (sleep 1s to avoid the next cargo run being a rebuild
            // again due to it being too close? don't bother.)
        }

        Ok(SourceReleaseTag { source_commit_id })
    }
}

#[derive(Debug)]
struct PushSourceToRemote {
    tag_name: String,
    remote_name: String,
}

#[derive(Debug)]
struct SourcePushed {
    source_commit_id: String,
}

impl Effect for PushSourceToRemote {
    type Requires = SourceReleaseTag;
    type Provides = SourcePushed;

    fn show_bullet_points(&self) -> String {
        let Self {
            tag_name,
            remote_name,
        } = self;
        format!(
            "  * in source repo: `git push {remote_name:?} {:?} {tag_name:?}`",
            SOURCE_CHECKOUT.branch_name
        )
    }

    fn run(self: Box<Self>, provided: Self::Requires) -> Result<Self::Provides> {
        let SourceReleaseTag { source_commit_id } = provided;
        SOURCE_CHECKOUT.git_working_dir().git_push(
            &self.remote_name,
            &[SOURCE_CHECKOUT.branch_name, &self.tag_name],
            false,
        )?;
        Ok(SourcePushed { source_commit_id })
    }
}

#[derive(Debug)]
struct Binary {
    target: CompilationTarget,
    program_name: &'static str,
}

#[derive(Debug)]
struct BinaryWithSha256sum {
    binary: Binary,
    binary_path: PathBuf,
    sha256sum: String,
}

impl BinaryWithSha256sum {
    /// Returns the target path it was copied to.
    fn copy_to_binaries_repo(&self, binaries_repo_base_dir: &Path) -> Result<PathBuf> {
        let Self {
            binary,
            binary_path,
            sha256sum: _,
        } = self;
        let Binary {
            target,
            program_name,
        } = binary;
        let CompilationTarget {
            target_triple,
            profile: _,
        } = target;
        // XX really ignore profile?
        let repo_section = if let Some(target_triple) = target_triple {
            BinariesRepoSection::from(target_triple)
        } else {
            BinariesRepoSection::from_local_os_and_arch()?
        };

        let binary_target_path = binaries_repo_base_dir
            .append(repo_section.installation_subpath())
            .append(program_name);

        create_dir_levels_if_necessary(
            binary_target_path
                .parent()
                .expect("file path has parent dir"),
            2,
        )?;
        std::fs::copy(&binary_path, &binary_target_path)
            .with_context(|| anyhow!("copying {binary_path:?} to {binary_target_path:?}",))?;
        Ok(binary_target_path)
    }
}

#[derive(Debug)]
struct BuildBinariesGetSha256sums {
    binaries: Vec<Binary>,
}

#[derive(Debug)]
struct BinariesWithSha256sum {
    source_commit_id: String,
    binaries_with_sha256sum: Vec<BinaryWithSha256sum>,
}

impl Effect for BuildBinariesGetSha256sums {
    // (`SourceReleaseTag` would suffice as requirement! But then the
    // processing chain wouldn't be linear.)
    type Requires = SourcePushed;
    type Provides = BinariesWithSha256sum;

    fn show_bullet_points(&self) -> String {
        let Self { binaries } = self;
        let binaries_string: String = binaries
            .into_iter()
            .map(
                |Binary {
                     target,
                     program_name,
                 }| { format!("      * {program_name:?} for {target}") },
            )
            .collect::<Vec<_>>()
            .join("\n");
        format!("  * build the following binaries and get their sha256sum:\n{binaries_string}")
    }

    fn run(self: Box<Self>, provided: Self::Requires) -> Result<Self::Provides> {
        let SourcePushed { source_commit_id } = provided;
        let BuildBinariesGetSha256sums { binaries } = *self;

        let binaries_with_sha256sum = binaries
            .into_iter()
            .map(|binary| -> Result<_> {
                let Binary {
                    target,
                    program_name,
                } = &binary;
                // Rebuild the binary, so that it picks up on the new Git
                // tag. We want that both for subsequent usage, but especially
                // so that it is up to date when copied off via
                // `ReleaseBinary`.
                target.run_build_in(SOURCE_CHECKOUT.working_dir_path(), program_name)?;

                let binary_path = SOURCE_CHECKOUT
                    .working_dir_path()
                    .append(target.subpath_to_binary(program_name));

                // Now that the binary is rebuilt, hash it; store errors,
                // complain about them later when actually needed (this will
                // be the case on Windows where the `sha256sum` command may
                // not be available, but we also don't publish the binary.)
                let sha256sum = sha256sum_paranoid(&binary_path)
                    .with_context(|| anyhow!("hashing file {binary_path:?}"))?;

                Ok(BinaryWithSha256sum {
                    binary,
                    binary_path,
                    sha256sum,
                })
            })
            .collect::<Result<_>>()?;
        Ok(BinariesWithSha256sum {
            source_commit_id,
            binaries_with_sha256sum,
        })
    }
}

#[derive(Debug)]
struct ReleaseBinaries<'t> {
    // Include this path since it comes from a *checked*
    // `BINARIES_CHECKOUT`, and hey, do not tie together variables
    // past the fact:
    binaries_checkout_working_dir: GitWorkingDir,
    source_version_tag: String,
    hostname: String,
    rustc_version: String,
    cargo_version: String,
    // The OS arch it is *built* on, not the arch of the
    // binary/binaries!
    build_os_arch: String,
    build_os_version: String,
    app_signature_private_key_path: PathBuf,
    app_signature_private_key: DebugIgnore<&'t AppSignaturePrivateKey>,
    sign: bool,
    local_user: Option<String>,
    push_binaries_to_remote: Option<String>,
}

#[derive(Debug)]
struct Done;

impl<'t> Effect for ReleaseBinaries<'t> {
    type Requires = BinariesWithSha256sum;
    type Provides = Done;

    fn show_bullet_points(&self) -> String {
        let Self {
            binaries_checkout_working_dir,
            source_version_tag,
            hostname: _,
            rustc_version: _,
            cargo_version: _,
            build_os_arch: _,
            build_os_version: _,
            app_signature_private_key_path,
            app_signature_private_key: _,
            sign,
            local_user,
            push_binaries_to_remote,
        } = self;
        let signing = if *sign {
            let key = local_user
                .as_ref()
                .expect("user is now required when signing");
            format!("* git tag the binaries repository with the PGP key {key}")
        } else {
            format!("- do *not* sign the binaries repository with a PGP key")
        };
        let push_remote = if let Some(remote) = push_binaries_to_remote {
            format!("* in binaries repository: `git push {remote:?}`")
        } else {
            format!("- do *not* git push binaries repository to upstream")
        };
        let binaries_checkout_working_dir_path =
            binaries_checkout_working_dir.working_dir_path_ref();
        format!(
            "  \
                 * copy the binaries into the right places below \
                 {binaries_checkout_working_dir_path:?}, create .info files, \
                 sign those with the private key from {app_signature_private_key_path:?}\n  \
                 * copy the `{CHANGELOG_FILE_NAME}` file from the source to the binaries repository\n  \
                 * run `git add .` in the binaries repository\n  \
                 * commit with a message mentioning source tag {source_version_tag:?}\n  \
                 {signing}\n  \
                 {push_remote}"
        )
    }

    fn run(self: Box<Self>, required: Self::Requires) -> Result<Self::Provides> {
        let ReleaseBinaries {
            binaries_checkout_working_dir,
            source_version_tag,
            hostname,
            rustc_version,
            cargo_version,
            build_os_arch,
            build_os_version,
            app_signature_private_key_path: _,
            app_signature_private_key,
            sign,
            local_user,
            push_binaries_to_remote,
        } = *self;
        let BinariesWithSha256sum {
            source_commit_id,
            binaries_with_sha256sum,
        } = required;

        // Had a 3rd entry, the sha256sum of the binary, but now there
        // are multiple. Use username instead? But would that *ever*
        // happen, multiple users on the same system used for building
        // binaries? No? So, just strip down to tag and hostname.
        let tag_name_if_signed = format!("{}-{}", source_version_tag, hostname);

        let creator = get_creator()?;
        let timestamp = get_timestamp();

        for binary_with_sha256sum in &binaries_with_sha256sum {
            let copied_to = binary_with_sha256sum
                .copy_to_binaries_repo(binaries_checkout_working_dir.working_dir_path_ref())?;

            let app_info = AppInfo {
                sha256: binary_with_sha256sum.sha256sum.clone(),
                version: source_version_tag.clone(),
                source_commit: source_commit_id.clone(),
                rustc_version: rustc_version.clone(),
                cargo_version: cargo_version.clone(),
                os_version: build_os_version.clone(),
                creator: creator.clone(),
                build_date: timestamp.clone(),
            };
            let app_info_path = app_info.save_for_app_path(&copied_to)?;

            // Sign that file.
            let app_info_contents = std::fs::read(&app_info_path)
                .with_context(|| anyhow!("reading app info file {app_info_path:?}"))?;
            let signature = app_signature_private_key.sign(&app_info_contents)?;
            signature.save_to_base(&app_info_path)?;
        }

        {
            let action = copy_file::<()>(
                &SOURCE_CHECKOUT
                    .working_dir_path()
                    .append(CHANGELOG_FILE_NAME),
                &BINARIES_CHECKOUT
                    .working_dir_path()
                    .append(CHANGELOG_FILE_NAME),
            );
            action.run(())?;
        }

        binaries_checkout_working_dir.git(
            // No worries about adding "." as check_status() was run
            // earlier, hence there are no other changes that might be
            // committed accidentally.
            &["add", "."],
            false,
        )?;

        let partial_commit_message = format!(
            "Version {source_version_tag}\n\
             \n\
             Source commit id: {source_commit_id}\n\
             \n\
             Details about the build host:\n\
             \n\
             - hostname: {hostname}\n\
             - rustc version{rustc_version}\n\
             - cargo version{cargo_version}\n\
             - OS / version: {build_os_version}\n\
             - arch: {build_os_arch}\n\
             \n\
             Created by make-release.rs"
        );

        // Commit message, too, in its subject line had the "SHA-256:
        // " part, so that people can see the sum right in the commit
        // message. But, how now that we commit multiple
        // binaries?. And there are now `.info` files, have them look
        // there, OK? -- And now self.partial_commit_message is
        // actually the full commit message, it starts with "Version
        // {}" which is perfect.
        let binary_commit_message = &partial_commit_message;

        BINARIES_CHECKOUT
            .git_working_dir()
            .git(&["commit", "-m", binary_commit_message], false)?;

        if self.sign {
            // Again, leave out the sha256 sum(s). OK?
            let tag_message_if_signed = format!("{}", partial_commit_message);

            if !BINARIES_CHECKOUT.git_working_dir().git_tag(
                &tag_name_if_signed,
                None,
                &tag_message_if_signed,
                sign,
                local_user.as_deref(),
            )? {
                println!(
                    "Note: git tag {tag_name_if_signed:?} already exists, \
                     linking to the same commit."
                );
            }
        }

        if let Some(remote_name) = &push_binaries_to_remote {
            // (Calculate this command beforehand and show as effect?
            // Ah can't, tag_name_if_signed can only be calculated in
            // this effect.)
            let mut branches_and_tags = vec![BINARIES_CHECKOUT.branch_name];
            if self.sign {
                branches_and_tags.push(&tag_name_if_signed)
            }
            BINARIES_CHECKOUT
                .git_working_dir()
                .git_push(remote_name, &branches_and_tags, false)?;
        }
        Ok(Done)
    }
}

fn main() -> Result<()> {
    let opts: Opts = Opts::from_args();

    let sign = if opts.sign && opts.no_sign {
        bail!("conflicting sign options given")
    } else {
        !opts.no_sign
    };

    let push_source = if opts.push_source && opts.no_push_source {
        bail!("conflicting push-source options given")
    } else {
        !opts.no_push_source
    };

    let push_binaries = if opts.push_binaries && opts.no_push_binaries {
        bail!("conflicting push-binary options given")
    } else {
        !opts.no_push_binaries
    };

    if sign {
        if opts.local_user.is_none() {
            if !opts.no_require_local_user {
                bail!(
                    "you want signing but have not provided the --local-user option; \
                     this will likely fail; if you want to try that, please add the \
                     --no-require-local-user option"
                )
            }
        }
    }

    let unless_dry_run = |res: Result<()>| -> Result<()> {
        match res {
            Ok(()) => Ok(()),
            Err(e) => {
                if opts.dry_run {
                    eprintln!("dry-run: would stop because of {e:#}");
                    Ok(())
                } else {
                    Err(e)
                }
            }
        }
    };

    println!("\n====Preparing...=============================================================\n");

    // Make sure the current dir is the top directory of the
    // xmlhub-indexer repo checkout
    {
        let cwd = current_dir()?;
        if cwd.file_name() != Some("xmlhub-indexer".as_ref()) {
            bail!(
                "the current directory is not the top-level directory of the \
                 xmlhub-indexer clone. `cd path/to/xmlhub-indexer` first."
            )
        }
    }

    // Check that we are on the correct branch etc.
    let source_checkout = SOURCE_CHECKOUT.check2(CheckExpectedSubpathsExist::Yes)?;
    // Check that everything is committed
    unless_dry_run(source_checkout.check_status())?;

    // Check that Cargo.toml does not refer to any packages by path,
    // as that would fail to compile on other people's machines (if
    // they don't have the source in the same locations; we are not
    // talking "cargo publish" which would remove the path directives,
    // but people using the clone of this repository directly!)
    unless_dry_run(check_cargo_toml_no_path("Cargo.toml"))?;

    // Check everything and run the test suite to make sure we are
    // ready for release.
    {
        run_cargo(source_checkout.working_dir_path(), &["check"])?;
        run_cargo(source_checkout.working_dir_path(), &["test"])?;
    }

    // Pass "--tags ..." as in `build.rs`, keeping in sync by sharing the code
    let args = &include!("../../include/git_describe_arguments.rs")[1..];
    let old_version: GitVersion<SemVersion> = source_checkout
        .git_working_dir()
        .git_describe(args)?
        .parse()
        .with_context(|| {
            anyhow!(
                "the version number from running `git describe` {args:?} in {:?} \
                     uses an invalid format",
                source_checkout.working_dir_path()
            )
        })?;

    let (new_version_tag_string, need_tag_in_any_case) = {
        if let Some((_depth, _sha1)) = &old_version.past_tag {
            let new_version = if opts.unchanged_output {
                old_version.version.next_compatible()
            } else {
                old_version.version.next_incompatible()
            };
            (format!("v{new_version}"), true)
        } else {
            (format!("v{}", old_version.version), false)
        }
    };

    // Then we will possibly change the changelog. Verify if it
    // already contains the release tag, too
    let changelog_tag = {
        let now = chrono::Local::now();
        let date = now.date_naive().to_string();
        ChangelogTag {
            tag_name: new_version_tag_string.clone(),
            date,
        }
    };

    let changelog_path = SOURCE_CHECKOUT
        .working_dir_path()
        .append(CHANGELOG_FILE_NAME);

    let changelog_has_release = {
        let mut lines = BufReader::new(File::open(&changelog_path)?).lines();
        let release_line = changelog_tag.release_line();
        lines.any(|line| {
            let line = line.expect("reading after opening failing is rare");
            let line = line.trim();
            line == release_line
        })
    };

    if !changelog_has_release {
        if !ask_yn("Have you updated `Changelog.md`? (You can do that while I wait.)")? {
            bail!("you need to do that first")
        }
    }

    let update_changelog_effect: Box<dyn Effect<Requires = (), Provides = UpdatedChangelog>> =
        if changelog_has_release {
            // Need to retrieve the source commit id *now*.
            let source_commit_id = source_checkout.git_working_dir().get_head_commit_id()?;
            NoOp::passing(
                {
                    let tag_name = changelog_tag.tag_name.clone();
                    move |()| UpdatedChangelog {
                        tag_name,
                        source_commit_id,
                    }
                },
                format!(
                    "Changelog file already contains the line {:?}",
                    changelog_tag.release_line()
                )
                .into(),
            )
        } else {
            // Will retrieve the source commit id after committing the
            // changelog change.
            Box::new(UpdateChangelogWithTag {
                changelog_path,
                changelog_tag,
            })
        };

    // if !changelog_has_release, we need a tag because the
    // UpdateChangelogWithTag action commits, hence, need a new tag --
    // OH, that will simply fail, though!! Remove the tag? For now
    // just let it fail when trying to make the tag again.
    let need_tag = need_tag_in_any_case || !changelog_has_release;

    let tag_effect: Box<dyn Effect<Requires = UpdatedChangelog, Provides = SourceReleaseTag>> =
        if need_tag {
            Box::new(CreateTag {
                sign,
                local_user: opts.local_user.clone(),
            })
        } else {
            NoOp::passing(
                |UpdatedChangelog {
                     source_commit_id,
                     tag_name: _,
                 }| SourceReleaseTag { source_commit_id },
                format!("using existing tag {new_version_tag_string:?}").into(),
            )
        };

    let push_to_remote: Box<dyn Effect<Requires = SourceReleaseTag, Provides = SourcePushed>> =
        if push_source {
            Box::new(PushSourceToRemote {
                tag_name: new_version_tag_string.clone(),
                remote_name: source_checkout.default_remote.clone(),
            })
        } else {
            NoOp::passing(
                |SourceReleaseTag { source_commit_id }| SourcePushed { source_commit_id },
                "do *not* push the source repository tag/branch \
                 because the --no-push-source option was given"
                    .into(),
            )
        };

    let build_binary = {
        let os = Os::from_local()?;
        let program_name = XMLHUB_BINARY_FILE_NAME;
        let profile = CompilationProfile::Release;

        // Currently hard-code compilation to intel + ARM on macOS,
        // and just the native platform on Linux.
        let binaries = match os {
            Os::MacOS => vec![
                Binary {
                    target: CompilationTarget {
                        target_triple: Some(TargetTriple {
                            arch: Arch::Aarch64,
                            os,
                            env: Env::None,
                        }),
                        profile,
                    },
                    program_name,
                },
                // Is it OK to explicitly target X86_64 even if
                // running there? Is it wasteful? But is it an
                // advantage for reproducibility? Let's see:
                Binary {
                    target: CompilationTarget {
                        target_triple: Some(TargetTriple {
                            arch: Arch::X86_64,
                            os,
                            env: Env::None,
                        }),
                        profile,
                    },
                    program_name,
                },
                // Cross-compile to Linux: besides rustup
                // --target=... requires `brew install lld`,
                // ~/.cargo/config.toml with `[target.$target] \n
                // linker = "lld"` (and perhaps TARGET_CC? no?)
                Binary {
                    target: CompilationTarget {
                        target_triple: Some(TargetTriple {
                            arch: Arch::X86_64,
                            os: Os::Linux,
                            env: Env::Musl,
                        }),
                        profile,
                    },
                    program_name,
                },
            ],
            Os::Linux => vec![Binary {
                target: CompilationTarget {
                    target_triple: None,
                    profile,
                },
                program_name,
            }],
        };
        Box::new(BuildBinariesGetSha256sums { binaries })
    };

    // Collect build information
    let rustc_version = stringify_error(prog_version(source_checkout.working_dir_path(), "rustc"));
    let cargo_version = stringify_error(prog_version(source_checkout.working_dir_path(), "cargo"));
    let hostname = hostname()?;
    let (os_arch, os_version) = {
        let info = os_info::get();
        (
            format!(
                "{} ({})",
                info.architecture().unwrap_or("-"),
                info.bitness()
            ),
            format!(
                "{} / {}, version {}, edition {}",
                info.os_type(),
                info.codename().unwrap_or("-"),
                info.version(),
                info.edition().unwrap_or("-")
            ),
        )
    };

    let app_signature_private_key;

    // Should we publish the binary?
    let release_binary: Box<dyn Effect<Requires = BinariesWithSha256sum, Provides = Done>> =
        if opts.no_publish_binary {
            NoOp::passing(
                |BinariesWithSha256sum {
                     source_commit_id: _,
                     binaries_with_sha256sum: _,
                 }| Done,
                "not publishing binary because --no-publish-binary option was given".into(),
            )
        } else {
            let app_signature_private_key_path = opts.app_private_key.ok_or_else(|| {
                anyhow!("when publishing the binary, `--app-private-key` is required")
            })?;
            app_signature_private_key =
                AppSignaturePrivateKey::load(&app_signature_private_key_path).with_context(
                    || anyhow!("loading private key from {app_signature_private_key_path:?}"),
                )?;

            // XX Bug: this check is too 'early', I mean it fetches
            // the remote even if !push. Not very important in
            // practise since we'll always have a remote, OK?
            let binaries_checkout = BINARIES_CHECKOUT.check2(CheckExpectedSubpathsExist::Yes)?;
            binaries_checkout.check_status()?;

            let push_binaries_to_remote = if push_binaries {
                // *Not* doing a git "pull" because that can lead to
                // merges, also it's unclear how security should be
                // treated: merging without requiring a tag signature
                // check could lead to unsafe contents merged and
                // subsequently signed.

                // But, we can do a "remote update" and complain about the
                // situation.
                {
                    // FUTURE: proper abstraction for running git
                    // directly on checkout contexts, also,
                    // abstraction for making error checks with
                    // dry_run easier? `unless_dry_run` from above
                    // does not transfer values, and wouldn't make
                    // sense. and the macro from xmlhub.rs doesn't
                    // apply here because we still want to run the
                    // commands.
                    match binaries_checkout.git_working_dir().git(
                        &["remote", "update", &binaries_checkout.default_remote],
                        false,
                    ) {
                        Ok(did_it) => {
                            if !did_it {
                                if opts.dry_run {
                                    eprintln!(
                                        "dry-run: would stop because git remote update \
                                         on the binaries repository was not successful"
                                    );
                                } else {
                                    bail!(
                                        "git remote update on the binaries repository \
                                         was not successful"
                                    )
                                }
                            }
                        }
                        Err(e) => {
                            if opts.dry_run {
                                eprintln!("dry-run: would stop because of {e:#}");
                            } else {
                                Err(e)?
                            }
                        }
                    }

                    let branch = binaries_checkout.branch_name;
                    let remote_branch = format!("{}/{}", binaries_checkout.default_remote, branch);
                    let remote_branch_is_ancestor = binaries_checkout.git_working_dir().git(
                        &["merge-base", "--is-ancestor", "--", &remote_branch, branch],
                        false,
                    )?;
                    if !remote_branch_is_ancestor {
                        bail!(
                            "the remote branch {remote_branch:?} is not an ancestor of \
                             (or the same as the) the local branch {branch:?} in \
                             the working directory {:?}",
                            binaries_checkout.working_dir_path(),
                        )
                    }
                }

                Some(binaries_checkout.default_remote.clone())
            } else {
                None
            };

            // _repo_section is now unused, since we get the paths
            // from target triple stuff instead now. We still check if
            // the OS is supported in the repository at all, though.
            if let Ok(_repo_section) = BinariesRepoSection::from_local_os_and_arch() {
                Box::new(ReleaseBinaries {
                    binaries_checkout_working_dir: binaries_checkout.git_working_dir(),
                    source_version_tag: new_version_tag_string.clone(),
                    hostname: hostname.clone(),
                    sign,
                    local_user: opts.local_user.clone(),
                    push_binaries_to_remote,
                    rustc_version,
                    cargo_version,
                    build_os_arch: os_arch,
                    build_os_version: os_version,
                    app_signature_private_key_path,
                    app_signature_private_key: (&app_signature_private_key).into(),
                })
            } else {
                NoOp::passing(
                    |BinariesWithSha256sum {
                         source_commit_id: _,
                         binaries_with_sha256sum: _,
                     }| Done,
                    "binaries are only published on macOS and Linux".into(),
                )
            }
        };

    let effect = bind(
        update_changelog_effect,
        bind(
            bind(bind(tag_effect, push_to_remote), build_binary),
            release_binary,
        ),
    );

    // We have finished collecting the effect(s). Now show and perhaps
    // run it/them.

    println!("\n====Effects:=================================================================\n");

    println!(
        "{}\n",
        if opts.verbose {
            effect.show()
        } else {
            effect.show_bullet_points()
        }
    );

    if opts.unchanged_output {
        println!(
            "-----------------------------------------------------------------------------\n\
             ! NOTE: you have given the --unchanged-output option, meaning that the \n\
             ! binary in this release ({new_version_tag_string}, up from {old_version}) purports to \n\
             ! produce outputs (index files) that are identical to the previous release. \n\
             ! If this is not true, stop here and remove that option!"
        );
    }

    println!("=============================================================================\n");

    let Done = if opts.dry_run {
        println!("Not running anything since --dry-run option was given.");
        Done
    } else if opts.yes {
        println!("Running these effects now.");
        effect.run(())?
    } else {
        if ask_yn("Should I run the above effects?")? {
            effect.run(())?
        } else {
            Done
        }
    };

    Ok(())
}
