use std::{
    env::current_dir,
    ffi::{OsStr, OsString},
    fmt::Debug,
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::PathBuf,
};

use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;

use debug_ignore::DebugIgnore;
use xmlhub_indexer::{
    cargo::check_cargo_toml_no_path,
    changelog::CHANGELOG_FILE_NAME,
    checkout_context::CheckExpectedSubpathsExist,
    command::{run, Capturing},
    const_util::file_name,
    effect::{bind, Effect, NoOp},
    git::{git, git_describe, git_push, git_stdout_string_trimmed, git_tag},
    git_version::{GitVersion, SemVersion},
    installation::app_info::AppInfo,
    installation::{
        app_signature::{AppSignaturePrivateKey, SaveLoadKeyFile},
        binaries_repo::BinariesRepoSection,
        json_file::JsonFile,
        util::{get_creator, get_timestamp},
    },
    path_util::AppendToPath,
    sha256::sha256sum_paranoid,
    util::{ask_yn, create_dir_levels_if_necessary, hostname, prog_version, stringify_error},
    xmlhub_indexer_defaults::{BINARIES_CHECKOUT, SOURCE_CHECKOUT, XMLHUB_INDEXER_BINARY_FILE},
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
    push_binary: bool,

    /// Do not push the branch and Git tag to the *binary*
    /// repository. The default is to push.
    #[clap(long)]
    no_push_binary: bool,
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
    tag_name: String,
}

impl Effect for UpdateChangelogWithTag {
    type Requires = ();

    type Provides = UpdatedChangelog;

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

        // Now also commit it. (Ah, passing around repositories in
        // globals, simply.)
        if !git(
            SOURCE_CHECKOUT.working_dir_path(),
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

        Ok(UpdatedChangelog {
            tag_name: changelog_tag.tag_name,
        })
    }
}

// Don't need to store tag_name String in here since it's constant and
// part of the individual contexts that need it.
#[derive(Debug)]
struct SourceReleaseTag;

/// Tag name is taken from UpdatedChangelog
#[derive(Debug)]
struct CreateTag {
    sign: bool,
    local_user: Option<String>,
}

impl Effect for CreateTag {
    type Requires = UpdatedChangelog;
    type Provides = SourceReleaseTag;

    fn run(self: Box<Self>, provided: Self::Requires) -> Result<Self::Provides> {
        git_tag(
            SOURCE_CHECKOUT.working_dir_path(),
            &provided.tag_name,
            None,
            "via make-release.rs",
            self.sign,
            self.local_user.as_deref(),
        )?;

        // Make sure cargo will rebuild the binary (see build.rs)
        {
            let path_str = ".released_version";
            let path = PathBuf::from(path_str);
            std::fs::write(&path, &provided.tag_name)
                .with_context(|| anyhow!("path should be writable: {path:?}"))?;
            // (sleep 1s to avoid the next cargo run being a rebuild
            // again due to it being too close? don't bother.)
        }

        Ok(SourceReleaseTag)
    }
}

#[derive(Debug)]
struct PushSourceToRemote {
    tag_name: String,
    remote_name: String,
}

#[derive(Debug)]
struct SourcePushed;

impl Effect for PushSourceToRemote {
    type Requires = SourceReleaseTag;
    type Provides = SourcePushed;

    fn run(self: Box<Self>, _provided: Self::Requires) -> Result<Self::Provides> {
        git_push(
            SOURCE_CHECKOUT.working_dir_path(),
            &self.remote_name,
            &[SOURCE_CHECKOUT.branch_name, &self.tag_name],
            false,
        )?;
        Ok(SourcePushed)
    }
}

#[derive(Debug)]
struct BuildBinaryAndSha256sum {}

#[derive(Debug)]
struct Sha256sumOfBinary {
    sha256sum: Result<String>,
}

impl Effect for BuildBinaryAndSha256sum {
    // (`SourceReleaseTag` would suffice as requirement! But then the
    // processing chain wouldn't be linear.)
    type Requires = SourcePushed;
    type Provides = Sha256sumOfBinary;

    fn run(self: Box<Self>, _provided: Self::Requires) -> Result<Self::Provides> {
        // Rebuild the binary, so that it picks up on the new Git
        // tag. We want that both for subsequent usage, but especially
        // so that it is up to date when copied off via
        // `ReleaseBinary`.
        cargo(&[
            "build",
            "--release",
            "--bin",
            file_name(XMLHUB_INDEXER_BINARY_FILE),
        ])?;

        // Now that the binary is rebuilt, hash it; store errors,
        // complain about them later when actually needed (this will
        // be the case on Windows where the `sha256sum` command may
        // not be available, but we also don't publish the binary.)
        let path = SOURCE_CHECKOUT
            .working_dir_path()
            .append(XMLHUB_INDEXER_BINARY_FILE);
        let sha256sum = sha256sum_paranoid(&path).with_context(|| anyhow!("hashing file {path:?}"));

        Ok(Sha256sumOfBinary { sha256sum })
    }
}

#[derive(Debug)]
struct ReleaseBinary<'t> {
    copy_binary_to: PathBuf,
    source_version_tag: String,
    hostname: String,
    source_commit: String,
    partial_commit_message: String,
    rustc_version: String,
    cargo_version: String,
    os_version: String,
    app_signature_private_key: DebugIgnore<&'t AppSignaturePrivateKey>,
    sign: bool,
    local_user: Option<String>,
    push_binary_to_remote: Option<String>,
}

#[derive(Debug)]
struct Done;

impl<'t> Effect for ReleaseBinary<'t> {
    type Requires = Sha256sumOfBinary;
    type Provides = Done;

    fn run(self: Box<Self>, required: Self::Requires) -> Result<Self::Provides> {
        let sha256sum = required.sha256sum?;

        let tag_name_if_signed = format!(
            "{}-{}-{}",
            self.source_version_tag, self.hostname, sha256sum
        );

        create_dir_levels_if_necessary(
            self.copy_binary_to
                .parent()
                .expect("file path has parent dir"),
            2,
        )?;

        std::fs::copy(XMLHUB_INDEXER_BINARY_FILE, &self.copy_binary_to).with_context(|| {
            anyhow!(
                "copying {XMLHUB_INDEXER_BINARY_FILE:?} to {:?}",
                self.copy_binary_to
            )
        })?;

        let creator = get_creator()?;
        let app_info = AppInfo {
            sha256: sha256sum.clone(),
            version: self.source_version_tag.clone(),
            source_commit: self.source_commit.clone(),
            rustc_version: self.rustc_version.clone(),
            cargo_version: self.cargo_version.clone(),
            os_version: self.os_version.clone(),
            creator,
            build_date: get_timestamp(),
        };
        let app_info_path = app_info.save_for_app_path(&self.copy_binary_to)?;

        // Sign that file.
        let app_info_contents = std::fs::read(&app_info_path)
            .with_context(|| anyhow!("reading app info file {app_info_path:?}"))?;
        let signature = self.app_signature_private_key.sign(&app_info_contents)?;
        signature.save_to_base(&app_info_path)?;

        git(
            BINARIES_CHECKOUT.working_dir_path(),
            // No worries about adding "." as check_status() was run
            // earlier, hence there are no other changes that might be
            // committed accidentally.
            &["add", "."],
            false,
        )?;

        let binary_commit_message = format!(
            "{} / SHA-256: {}\n\n{}",
            self.source_version_tag, sha256sum, self.partial_commit_message
        );

        git(
            BINARIES_CHECKOUT.working_dir_path(),
            &["commit", "-m", &binary_commit_message],
            false,
        )?;

        if self.sign {
            let tag_message_if_signed = format!(
                "{}\n\nsha256sum: {}",
                self.partial_commit_message, sha256sum
            );

            if !git_tag(
                BINARIES_CHECKOUT.working_dir_path(),
                &tag_name_if_signed,
                None,
                &tag_message_if_signed,
                self.sign,
                self.local_user.as_deref(),
            )? {
                println!(
                    "Note: git tag {tag_name_if_signed:?} already exists, \
                     linking to the same commit."
                );
            }
        }

        if let Some(remote_name) = &self.push_binary_to_remote {
            // (Calculate this command beforehand and show as effect?
            // Ah can't, tag_name_if_signed can only be calculated in
            // this effect.)
            let mut branches_and_tags = vec![BINARIES_CHECKOUT.branch_name];
            if self.sign {
                branches_and_tags.push(&tag_name_if_signed)
            }
            git_push(
                BINARIES_CHECKOUT.working_dir_path(),
                remote_name,
                &branches_and_tags,
                false,
            )?;
        }
        Ok(Done)
    }
}

fn cargo<S: AsRef<OsStr> + Debug>(args: &[S]) -> Result<bool> {
    run(
        SOURCE_CHECKOUT.working_dir_path(),
        "cargo",
        args,
        &[],
        &[0],
        Capturing::none(),
    )
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

    let push_binary = if opts.push_binary && opts.no_push_binary {
        bail!("conflicting push-binary options given")
    } else {
        !opts.no_push_binary
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
        cargo(&["check"])?;
        cargo(&["test"])?;
    }

    // Pass "--tags ..." as in `build.rs`, keeping in sync by sharing the code
    let args = &include!("../../include/git_describe_arguments.rs")[1..];
    let old_version: GitVersion<SemVersion> =
        git_describe(source_checkout.working_dir_path(), args)?
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
        if !ask_yn("Have you updated `Changelog.md`?")? {
            bail!("you need to do that first")
        }
    }

    let update_changelog_effect: Box<dyn Effect<Requires = (), Provides = UpdatedChangelog>> =
        if changelog_has_release {
            NoOp::providing(
                UpdatedChangelog {
                    tag_name: changelog_tag.tag_name.clone(),
                },
                format!(
                    "Changelog file already contains the line {:?}",
                    changelog_tag.release_line()
                )
                .into(),
            )
        } else {
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
            NoOp::providing(
                SourceReleaseTag,
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
            NoOp::providing(
                SourcePushed,
                "not pushing tag/branch because the --no-push option was given".into(),
            )
        };

    let build_binary = Box::new(BuildBinaryAndSha256sum {});

    // Collect build information
    let commit_id =
        git_stdout_string_trimmed(source_checkout.working_dir_path(), &["rev-parse", "HEAD"])?;
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
    let release_binary: Box<dyn Effect<Requires = Sha256sumOfBinary, Provides = Done>> =
        if opts.no_publish_binary {
            NoOp::providing(
                Done,
                "not publishing binary because --no-publish-binary option was given".into(),
            )
        } else {
            let app_private_key = &opts.app_private_key.ok_or_else(|| {
                anyhow!("when publishing the binary, `--app-private-key` is required")
            })?;
            app_signature_private_key = AppSignaturePrivateKey::load(&app_private_key)
                .with_context(|| anyhow!("loading private key from {app_private_key:?}"))?;

            // XX Bug: this check is too 'early', I mean it fetches
            // the remote even if !push. Not very important in
            // practise since we'll always have a remote, OK?
            let binaries_checkout = BINARIES_CHECKOUT.check2(CheckExpectedSubpathsExist::Yes)?;
            binaries_checkout.check_status()?;

            let push_binary_to_remote = if push_binary {
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
                    match git(
                        binaries_checkout.working_dir_path(),
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
                    let remote_branch_is_ancestor = git(
                        binaries_checkout.working_dir_path(),
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

            if let Ok(repo_section) = BinariesRepoSection::from_local_os_and_arch() {
                let binary_target_path = binaries_checkout
                    .working_dir_path()
                    .append(repo_section.installation_subpath())
                    .append(file_name(XMLHUB_INDEXER_BINARY_FILE));

                let partial_commit_message = format!(
                    "Version {new_version_tag_string}\n\
                     \n\
                     Source commit id: {commit_id}\n\
                     \n\
                     Details about the build host:\n\
                     \n\
                     - hostname: {hostname}\n\
                     - rustc version{rustc_version}\n\
                     - cargo version{cargo_version}\n\
                     - OS / version: {os_version}\n\
                     - arch: {os_arch}\n\
                     \n\
                     Created by make-release.rs"
                );

                Box::new(ReleaseBinary {
                    copy_binary_to: binary_target_path,
                    source_version_tag: new_version_tag_string.clone(),
                    hostname: hostname.clone(),
                    partial_commit_message,
                    sign,
                    local_user: opts.local_user.clone(),
                    push_binary_to_remote,
                    source_commit: commit_id,
                    rustc_version,
                    cargo_version,
                    os_version,
                    app_signature_private_key: (&app_signature_private_key).into(),
                })
            } else {
                NoOp::providing(
                    Done,
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

    println!("{}", effect.show());

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
