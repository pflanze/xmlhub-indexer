use std::{ffi::OsString, path::PathBuf};

use anyhow::{anyhow, Result};

use crate::{
    checkout_context::{CheckExpectedSubpathsExist, CheckoutContext},
    fixup_path::FixupPath,
    git::git,
    git_version::{GitVersion, SemVersion},
    path_util::AppendToPath,
    xmlhub_global_opts::{DrynessOpt, VersionCheckOpt},
    xmlhub_indexer_defaults::{
        git_log_version_checker, XMLHUB_CHECKOUT, XMLHUB_EXPERIMENTS_CHECKOUT,
    },
};

#[derive(clap::Parser, Debug)]
pub struct CloneToOpts {
    #[clap(flatten)]
    pub dryness: DrynessOpt,
    #[clap(flatten)]
    pub versioncheck: VersionCheckOpt,

    /// Do not show the Git commands that are run (by default, they
    /// are shown even if the global `--verbose` option was not given)
    #[clap(long)]
    pub no_verbose: bool,

    /// Instead of the official XML Hub repository, clone the
    /// `xmlhub-experiments` repository for experimenting with
    #[clap(long)]
    pub experiments: bool,

    /// Where to create the repository clone. If the given path is
    /// pointing to an existing directory, the name of the upstream
    /// repository ("xmlhub" or "xmlhub-experiments") is added to the
    /// path. Otherwise the path is taken as desired path to the
    /// future directory ("base path") of the Git checkout, i.e. the
    /// repository is renamed to the last segment. (I.e. this works
    /// similar to how the unix `cp` or `mv` commands work.)
    pub target_path: Option<PathBuf>,
}

#[derive(Debug)]
struct Target {
    checkout: CheckoutContext<'static, PathBuf>,
    needs_cloning: bool,
}

impl Target {
    /// Check 3 cases: (a) target_path does not exist -> use directly;
    /// (b) target_path does exist and is a git repo -> only
    /// configure; (c) target_path does exist and is a normal dir ->
    /// add checkout's upstream repo name. Note: only does a check1
    /// with CheckExpectedSubpathsExist::No, as the check2 should be
    /// later to point out user error.
    fn new(
        checkout: CheckoutContext<'static, &'static str>,
        target_path: PathBuf,
        exists: bool,
    ) -> Self {
        let checkout = checkout.replace_working_dir_path(target_path);
        if !exists {
            // User gave full non-existing base_path to clone to
            Self {
                checkout,
                needs_cloning: true,
            }
        } else if let Ok(_) = (&checkout).clone().check1(CheckExpectedSubpathsExist::No) {
            // User gave full base_path to existing clone to configure
            Self {
                checkout,
                needs_cloning: false,
            }
        } else {
            // User gave path to parent directory to put the target inside
            Self {
                checkout: checkout.replace_working_dir_path(
                    checkout
                        .working_dir_path()
                        .append(checkout.supposed_upstream_repo_name())
                        .into(),
                ),
                needs_cloning: true,
            }
        }
    }
}

/// Execute a `clone-to` command.
pub fn clone_to_command(
    program_version: GitVersion<SemVersion>,
    command_opts: CloneToOpts,
) -> Result<()> {
    let CloneToOpts {
        dryness: DrynessOpt { dry_run },
        versioncheck: VersionCheckOpt { no_version_check },
        no_verbose,
        target_path,
        experiments,
    } = command_opts;

    let target_path =
        target_path.ok_or_else(|| anyhow!("missing BASE_PATH argument. Run --help for help."))?;

    let target = {
        let checkout = if experiments {
            XMLHUB_EXPERIMENTS_CHECKOUT
        } else {
            XMLHUB_CHECKOUT
        };
        let exists = target_path.exists();
        Target::new(checkout, target_path, exists)
    };

    // Define a macro to only run $body if opts.dry_run is false,
    // otherwise show $message instead, or show $message anyway if
    // command_opts.no_verbose is false.
    macro_rules! check_dry_run {
        { message: $message:expr, $body:expr } => {
            let s = || -> String { $message.into() };
            if dry_run {
                crate::dry_run::eprintln_dry_run(s());
            } else {
                if ! no_verbose {
                    crate::dry_run::eprintln_running(s());
                }
                $body;
            }
        }
    }

    if !target.needs_cloning {
        eprintln!(
            "git checkout at {:?} already exists, just configuring it",
            target.checkout.working_dir_path()
        );
    } else {
        let parent_dir = target
            .checkout
            .working_dir_path()
            .parent()
            .ok_or_else(
                // This only happens for the path "". (XX *not* for "foo", right?)
                || {
                    anyhow!(
                        "the given path {:?} has no parent directory",
                        target.checkout.working_dir_path()
                    )
                },
            )?
            .fixup();

        let url = target.checkout.supposed_upstream_git_url;

        let subfolder_name = target
            .checkout
            .working_dir_path()
            .file_name()
            .ok_or_else(|| {
                anyhow!(
                    "the given path {:?} is missing the subdirectory name",
                    target.checkout.working_dir_path()
                )
            })?;

        check_dry_run! {
            message: format!("cd {:?} && git clone {:?} {:?}",
                             parent_dir,
                             url,
                             subfolder_name),
            git(
                &parent_dir,
                &[ OsString::from("clone"), OsString::from(url), subfolder_name.into() ],
                false
            )?
        }
    }

    if !dry_run {
        let check = if !target.needs_cloning {
            CheckExpectedSubpathsExist::Yes
        } else {
            // Do not check subpaths here, as the `clone-to`
            // subcommand knows the correct repository to clone from,
            // if that doesn't contain the expected files, so be it,
            // don't give an error. (Probably error will happen down
            // the line anyway, though!)
            CheckExpectedSubpathsExist::No
        };
        let _ = target.checkout.clone().check1(check)?;
    }

    check_dry_run! {
        message: format!("cd {:?} && git config pull.rebase false",
                         target.checkout.working_dir_path()),
        git(
            &target.checkout.working_dir_path(),
            &[ "config", "pull.rebase", "false" ],
            false
        )?
    }

    // Check that we are up to dealing with this repository, OK?
    let git_log_version_checker = git_log_version_checker(
        program_version,
        no_version_check,
        target.checkout.working_dir_path(),
    );

    git_log_version_checker.check_git_log()?;

    Ok(())
}
