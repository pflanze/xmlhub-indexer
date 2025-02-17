use std::{env::current_dir, ffi::OsStr, fmt::Debug, path::PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;

use xmlhub_indexer::{
    cargo::check_cargo_toml_no_path,
    command::{run, Capturing},
    const_util::file_name,
    effect::{bind, Effect, NoOp},
    git::{git, git_describe, git_push, git_stdout_string_trimmed, git_tag},
    git_version::{GitVersion, SemVersion},
    util::{
        ask_yn, create_dir_levels_if_necessary, hostname, prog_version, sha256sum, stringify_error,
    },
    xmlhub_indexer_defaults::{BINARIES_CHECKOUT, SOURCE_CHECKOUT, XMLHUB_INDEXER_BINARY_FILE},
};

#[derive(clap::Parser, Debug)]
#[clap(next_line_help = true)]
/// Produce a new release of the xmlhub-indexer repository, both the
/// source code as well as a `xmlhub-build-index` binary if on macOS
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
    /// `xmlhub-build-index` are not created differently due to changes in
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

    /// Sign the Git tags with your PGP key, via the git tag -s option
    #[clap(long)]
    sign: bool,

    /// When signing, the name of the key to use. This is passed on to
    /// `git tag`, which passes it to `gpg`. The key fingerprint works
    /// best. If not given, signing assumes the default key name,
    /// which may fail if there are multiple keys.
    #[clap(long)]
    local_user: Option<String>,

    /// Push the branch and Git tag to the default remote (presumed to
    /// be the upstream repository).
    #[clap(long)]
    push: bool,
}

// Don't need to store tag_name String in here since it's constant and
// part of the individual contexts that need it.
#[derive(Debug)]
struct SourceReleaseTag;

#[derive(Debug)]
struct CreateTag {
    tag_name: String,
    sign: bool,
    local_user: Option<String>,
}

impl Effect for CreateTag {
    type Requires = ();
    type Provides = SourceReleaseTag;

    fn run(self: Box<Self>, _provided: Self::Requires) -> Result<Self::Provides> {
        git_tag(
            SOURCE_CHECKOUT.working_dir_path(),
            &self.tag_name,
            None,
            "via make-xmlhub-indexer-release.rs",
            self.sign,
            self.local_user.as_deref(),
        )?;

        // Make sure cargo will rebuild the binary (see build.rs)
        {
            let path_str = ".released_version";
            let path = PathBuf::from(path_str);
            std::fs::write(&path, &self.tag_name)
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
        let sha256sum = sha256sum(
            SOURCE_CHECKOUT.working_dir_path(),
            XMLHUB_INDEXER_BINARY_FILE,
        );

        Ok(Sha256sumOfBinary { sha256sum })
    }
}

#[derive(Debug)]
struct ReleaseBinary {
    copy_binary_to: PathBuf,
    source_version_tag: String,
    hostname: String,
    partial_commit_message: String,
    sign: bool,
    local_user: Option<String>,
    push_to_remote: Option<String>,
}

#[derive(Debug)]
struct Done;

impl Effect for ReleaseBinary {
    type Requires = Sha256sumOfBinary;
    type Provides = Done;

    fn run(self: Box<Self>, required: Self::Requires) -> Result<Self::Provides> {
        let sha256sum = required.sha256sum?;

        let tag_name_if_signed = format!(
            "{}-{}-{}",
            self.source_version_tag, self.hostname, sha256sum
        );

        create_dir_levels_if_necessary(
            &self
                .copy_binary_to
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

        let binary_commit_message = format!(
            "{} / SHA-256: {}\n\n{}",
            self.source_version_tag, sha256sum, self.partial_commit_message
        );

        let tag_message_if_signed = format!(
            "{}\n\nsha256sum: {}",
            self.partial_commit_message, sha256sum
        );

        git(
            BINARIES_CHECKOUT.working_dir_path(),
            // No worries about adding "." as check_status() was run
            // earlier, hence there are no other changes that might be
            // committed accidentally.
            &["add", "."],
            false,
        )?;

        git(
            BINARIES_CHECKOUT.working_dir_path(),
            &["commit", "-m", &binary_commit_message],
            false,
        )?;

        if self.sign {
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

        if let Some(remote_name) = &self.push_to_remote {
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
    let opts = Opts::from_args();

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

    // Check that we are on the correct branch
    SOURCE_CHECKOUT.check_current_branch()?;
    // Check that everything is committed
    unless_dry_run(SOURCE_CHECKOUT.check_status())?;

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
        git_describe(SOURCE_CHECKOUT.working_dir_path(), args)?
            .parse()
            .with_context(|| {
                anyhow!(
                    "the version number from running `git describe` {args:?} in {:?} \
                     uses an invalid format",
                    SOURCE_CHECKOUT.working_dir_path()
                )
            })?;

    let (new_version_tag_string, need_tag) = {
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
    let tag_effect: Box<dyn Effect<Requires = (), Provides = SourceReleaseTag>> = if need_tag {
        Box::new(CreateTag {
            tag_name: new_version_tag_string.clone(),
            sign: opts.sign,
            local_user: opts.local_user.clone(),
        })
    } else {
        NoOp::providing(
            SourceReleaseTag,
            format!("using existing tag {new_version_tag_string:?}").into(),
        )
    };

    let push_to_remote: Box<dyn Effect<Requires = SourceReleaseTag, Provides = SourcePushed>> =
        if opts.push {
            Box::new(PushSourceToRemote {
                tag_name: new_version_tag_string.clone(),
                remote_name: SOURCE_CHECKOUT.git_remote_get_default()?,
            })
        } else {
            NoOp::providing(
                SourcePushed,
                "not pushing tag/branch because --push option was not given".into(),
            )
        };

    let build_binary = Box::new(BuildBinaryAndSha256sum {});

    // Collect build information
    let commit_id =
        git_stdout_string_trimmed(SOURCE_CHECKOUT.working_dir_path(), &["rev-parse", "HEAD"])?;
    let rustc_version = stringify_error(prog_version(SOURCE_CHECKOUT.working_dir_path(), "rustc"));
    let cargo_version = stringify_error(prog_version(SOURCE_CHECKOUT.working_dir_path(), "cargo"));
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

    // Should we publish the binary?
    let release_binary: Box<dyn Effect<Requires = Sha256sumOfBinary, Provides = Done>> =
        if opts.no_publish_binary {
            NoOp::providing(
                Done,
                "not publishing binary because --no-publish-binary option was given".into(),
            )
        } else {
            if !BINARIES_CHECKOUT.checkout_dir_exists() {
                bail!(
                    "missing the git working directory at {:?}; \
                       please run: `cd ..; git clone {}; cd -`",
                    BINARIES_CHECKOUT.working_dir_path,
                    BINARIES_CHECKOUT.supposed_upstream_git_url
                )
            }
            BINARIES_CHECKOUT.check_current_branch()?;
            BINARIES_CHECKOUT.check_status()?;

            let push_to_remote = if opts.push {
                Some(BINARIES_CHECKOUT.git_remote_get_default()?)
            } else {
                None
            };

            let in_dir = |dir_segments: &[&str]| {
                let mut binary_target_path = PathBuf::from(BINARIES_CHECKOUT.working_dir_path());
                for segment in dir_segments {
                    binary_target_path.push(segment)
                }
                binary_target_path.push(file_name(XMLHUB_INDEXER_BINARY_FILE));

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
                     Created by make-xmlhub-indexer-release.rs"
                );

                Box::new(ReleaseBinary {
                    copy_binary_to: binary_target_path,
                    source_version_tag: new_version_tag_string.clone(),
                    hostname: hostname.clone(),
                    partial_commit_message,
                    sign: opts.sign,
                    local_user: opts.local_user.clone(),
                    push_to_remote,
                })
            };
            match std::env::consts::OS {
                "macos" => in_dir(&["macOS", std::env::consts::ARCH]),
                "linux" => in_dir(&["linux", std::env::consts::ARCH]),
                _ => NoOp::providing(
                    Done,
                    "binaries are only published on macOS and Linux".into(),
                ),
            }
        };

    let effect = bind(
        bind(bind(tag_effect, push_to_remote), build_binary),
        release_binary,
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
