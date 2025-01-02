use std::{
    env::current_dir,
    ffi::OsStr,
    fmt::Debug,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;

use xmlhub_indexer::{
    command::run,
    git::{
        git, git_branch_show_current, git_describe, git_remote_get_default_for_branch, git_status,
        git_stdout_string_trimmed, git_tag,
    },
    git_version::{GitVersion, SemVersion},
    util::{
        ask_yn, create_dir_levels_if_necessary, file_name, hostname, prog_version, sha256sum,
        stringify_error,
    },
};

#[derive(clap::Parser, Debug)]
/// Produce a new release of the xmlhub-indexer program, both the
/// source code as well as a binary if on macOS. Also creates a new
/// version tag if necessary.
struct Opts {
    /// By default, the program shows the actions to be carried out
    /// and then asks for confirmation. This option ends the program
    /// without asking for confirmation or carrying them out.
    #[clap(long)]
    dry_run: bool,

    /// Carry out the actions without asking for confirmation
    /// (basically the inverse of --dry-run).
    #[clap(long)]
    yes: bool,

    /// Use this option if you're confident that the files written by
    /// xmlhub-indexer are not created differently due to changes in
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

struct CheckoutContext {
    working_dir_path: &'static str,
    /// The name of the branch used in the remote repository, also
    /// expected to be the same in the local checkout
    branch_name: &'static str,
}

impl CheckoutContext {
    fn working_dir_path(&self) -> &Path {
        self.working_dir_path.as_ref()
    }

    fn check_current_branch(&self) -> Result<()> {
        let current_branch = git_branch_show_current(self.working_dir_path())?;
        if current_branch.as_deref() != Some(self.branch_name) {
            bail!(
                "expecting checked-out branch to be `{}`, but it is `{}`",
                self.branch_name,
                current_branch
                    .as_deref()
                    .unwrap_or("none, i.e. detached head")
            )
        }
        Ok(())
    }

    fn check_status(&self) -> Result<()> {
        let items = git_status(self.working_dir_path())?;
        if !items.is_empty() {
            bail!(
                "uncommitted changes in the git checkout at {:?}: {items:?}",
                self.working_dir_path()
            );
        }
        Ok(())
    }

    fn git_remote_get_default(&self) -> Result<String> {
        git_remote_get_default_for_branch(self.working_dir_path(), self.branch_name)?.ok_or_else(
            || {
                anyhow!(
                    "branch {:?} in {:?} does not have a default remote set, \
                     you can't use the `--push` option because of that",
                    self.branch_name,
                    self.working_dir_path()
                )
            },
        )
    }
}

const XMLHUB_INDEXER_BINARY_FILE: &str = "target/release/xmlhub-indexer";

const SOURCE_CHECKOUT: CheckoutContext = CheckoutContext {
    working_dir_path: ".",
    branch_name: "master",
};

const BINARIES_CHECKOUT: CheckoutContext = CheckoutContext {
    working_dir_path: "../xmlhub-indexer-binaries/",
    branch_name: "master",
};

/// make-xmlhub-indexer-release is first collecting the data into effects to be
/// carried out, then asks if those should be run, then runs
/// them. This is the interface for an effect. Note that an Effect can
/// implicitly depend on another Effect being run first!
trait Effect: Debug {
    fn run(&self) -> Result<()>;
}

#[derive(Debug)]
struct CreateTag {
    tag_name: String,
    sign: bool,
    local_user: Option<String>,
    push_to_remote: Option<String>,
}

impl Effect for CreateTag {
    fn run(&self) -> Result<()> {
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

        if let Some(remote_name) = &self.push_to_remote {
            git(
                SOURCE_CHECKOUT.working_dir_path(),
                &[
                    "push",
                    remote_name,
                    SOURCE_CHECKOUT.branch_name,
                    &self.tag_name,
                ],
            )?;
        }

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

        Ok(())
    }
}

/// Note: depends on CreateTag being run first!
#[derive(Debug)]
struct ReleaseBinary {
    copy_binary_to: PathBuf,
    source_version_tag: String,
    hostname: String,
    commit_message: String,
    sign: bool,
    local_user: Option<String>,
    push_to_remote: Option<String>,
}

impl Effect for ReleaseBinary {
    fn run(&self) -> Result<()> {
        // This depends on CreateTag being run first
        let sha256sum = sha256sum(
            SOURCE_CHECKOUT.working_dir_path(),
            XMLHUB_INDEXER_BINARY_FILE,
        )?;

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
            self.source_version_tag, sha256sum, self.commit_message
        );

        let tag_message_if_signed = format!("{}\n\nsha256sum: {}", self.commit_message, sha256sum);

        git(
            BINARIES_CHECKOUT.working_dir_path(),
            // No worries about adding "." as check_status() was run
            // earlier, hence there are no other changes that might be
            // committed accidentally.
            &["add", "."],
        )?;

        git(
            BINARIES_CHECKOUT.working_dir_path(),
            &["commit", "-m", &binary_commit_message],
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
            // (Calculate this beforehand and show as effect? Ah
            // can't, tag_name_if_signed can only be calculated in
            // this effect.)
            let mut args = vec!["push", remote_name, BINARIES_CHECKOUT.branch_name];
            if self.sign {
                args.push(&tag_name_if_signed)
            }
            git(BINARIES_CHECKOUT.working_dir_path(), &args)?;
        }
        Ok(())
    }
}

fn cargo<S: AsRef<OsStr> + Debug>(args: &[S]) -> Result<bool> {
    run(SOURCE_CHECKOUT.working_dir_path(), "cargo", args, &[], &[0])
}

fn main() -> Result<()> {
    let opts = Opts::from_args();

    // Make sure the current dir is the top directory of the
    // xmlhub-indexer repo checkout
    {
        let cwd = current_dir()?;
        if cwd.file_name() != Some("xmlhub-indexer".as_ref()) {
            bail!(
                "current directory is not the top-level directory of the \
                 xmlhub-indexer clone. `cd path/to/xmlhub-indexer` first."
            )
        }
    }

    // Check that we are on the correct branch
    SOURCE_CHECKOUT.check_current_branch()?;
    // Check that everything is committed
    SOURCE_CHECKOUT.check_status()?;

    // Check everything and run the test suite to make sure we are
    // ready for release.
    {
        cargo(&["check"])?;
        cargo(&["test"])?;
    }

    let mut effects: Vec<Box<dyn Effect>> = Vec::new();

    let (new_version_tag_string, need_tag) = {
        // Pass "--tags" as in `xmlhub-indexer.rs`, keep in sync!
        let current_version: GitVersion<SemVersion> =
            git_describe(SOURCE_CHECKOUT.working_dir_path(), &["--tags"])?
                .parse()
                .with_context(|| {
                    anyhow!(
                        "the version number from running `git describe --tags` in {:?} \
                         uses an invalid format",
                        SOURCE_CHECKOUT.working_dir_path()
                    )
                })?;

        if let Some((_depth, _sha1)) = &current_version.past_tag {
            let new_version = if opts.unchanged_output {
                current_version.version.next_minor()
            } else {
                current_version.version.next_major()
            };
            (format!("v{new_version}"), true)
        } else {
            (format!("v{}", current_version.version), false)
        }
    };
    if need_tag {
        let push_to_remote = if opts.push {
            Some(SOURCE_CHECKOUT.git_remote_get_default()?)
        } else {
            None
        };

        effects.push(Box::new(CreateTag {
            tag_name: new_version_tag_string.clone(),
            sign: opts.sign,
            local_user: opts.local_user.clone(),
            push_to_remote,
        }));
    }

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
    if opts.no_publish_binary {
        ()
    } else {
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

            let commit_message = format!(
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

            effects.push(Box::new(ReleaseBinary {
                copy_binary_to: binary_target_path,
                source_version_tag: new_version_tag_string.clone(),
                hostname: hostname.clone(),
                commit_message,
                sign: opts.sign,
                local_user: opts.local_user.clone(),
                push_to_remote,
            }));
        };
        match std::env::consts::OS {
            "macos" => in_dir(&["macOS", std::env::consts::ARCH]),
            "linux" => in_dir(&["linux", std::env::consts::ARCH]),
            _ => (),
        }
    }

    // We have finished collecting the effects. Now show and perhaps
    // run them.
    let effects_string = format!(
        "{effects:#?}{}\n",
        if opts.unchanged_output {
            "\n\n! NOTE: you have given the --unchanged-output option, meaning \n\
             ! that the binary from this release purports to produce outputs \n\
             ! (index files) that are identical to the previous release. \n\
             ! If this is not true, stop here and remove that option!"
        } else {
            ""
        }
    );
    let run_effects = || {
        for effect in effects {
            effect.run()?;
        }
        Ok(())
    };
    if opts.dry_run {
        println!("\nWould run these effects:\n{effects_string}");
        Ok(())
    } else if opts.yes {
        println!("\nRunning these effects:\n{effects_string}");
        run_effects()
    } else {
        println!("\n{effects_string}");
        if ask_yn("Should I run the above effects?")? {
            run_effects()
        } else {
            Ok(())
        }
    }
}
