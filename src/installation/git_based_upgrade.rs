//! Upgrading an executable by (cloning and) pulling from a Git
//! repository containing signed binaries.

use std::{
    cmp::Ordering,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, Context, Result};

use crate::{
    changelog::{Changelog, ChangelogDisplay, ChangelogDisplayStyle, CHANGELOG_FILE_NAME},
    git::git,
    git_version::{GitVersion, SemVersion},
    installation::shell::AppendToShellFileDone,
    path_util::AppendToPath,
    sha256::sha256sum,
    util::ask_yn,
    xmlhub_global_opts::PROGRAM_NAME,
    xmlhub_indexer_defaults::{BINARIES_CHECKOUT, XMLHUB_BINARY_FILE_NAME},
};

use super::{
    app_info::AppInfo,
    app_signature::{AppSignature, SaveLoadKeyFile},
    binaries_repo::BinariesRepoSection,
    defaults::global_app_state_dir,
    install::install_executable,
    trusted_keys::get_trusted_key,
};

pub struct VerifiedExecutable {
    pub binary_path: PathBuf,
    pub app_info: AppInfo,
    pub changelog_path: PathBuf,
}

// Todo: change to git remote update and reset, so that trimming the
// upstream repository every now and then would be possible?
pub fn pull_verified_executable() -> Result<VerifiedExecutable> {
    let binaries_repo_name = "xmlhub-indexer-binaries";

    let binaries_checkout = BINARIES_CHECKOUT.replace_working_dir_path(
        global_app_state_dir()?
            .clones_base()?
            .append(binaries_repo_name),
    );

    if binaries_checkout.working_dir_path().is_dir() {
        println!("Updating the {binaries_repo_name} repository via git pull.");
        git(binaries_checkout.working_dir_path(), &["pull"], false)?;
    } else {
        println!("Cloning the {binaries_repo_name} repository.");
        let parent_dir = binaries_checkout
            .working_dir_path()
            .parent()
            .expect("dir created by appending, so parent must exist");
        // ^ XX actually no, if HOME="", right?
        let subdir = binaries_checkout
            .working_dir_path()
            .file_name()
            .expect("ditto");
        // ^ XX ditto
        git(
            &parent_dir,
            &[
                "clone".into(),
                (&binaries_checkout.supposed_upstream_git_url).into(),
                subdir.to_owned(),
            ],
            false,
        )?;
    }

    let repo_section = BinariesRepoSection::from_local_os_and_arch()?;
    let binary_path = binaries_checkout
        .working_dir_path()
        .append(repo_section.installation_subpath())
        .append(XMLHUB_BINARY_FILE_NAME);
    let (app_info, info_path, info_bytes) = AppInfo::load_for_app_path(&binary_path)?;
    let sig = AppSignature::load_from_base(&info_path)?;
    let (is_valid, public_key) = sig.verify(&info_bytes)?;
    if is_valid {
        if let Some(trusted_key) = get_trusted_key(&public_key) {
            println!(
                "Good info signature made with {trusted_key} on {}",
                sig.metadata.birth
            );
            let actual_hash = sha256sum(&binary_path).with_context(|| anyhow!(""))?;
            if actual_hash == app_info.sha256 {
                println!("App file hash is valid.");
                let changelog_path = binaries_checkout
                    .working_dir_path()
                    .append(CHANGELOG_FILE_NAME);
                Ok(VerifiedExecutable {
                    binary_path,
                    app_info,
                    changelog_path,
                })
            } else {
                bail!(
                    "invalid file hash: the file {binary_path:?} hashes to {actual_hash:?}, \
                     but its signed info file expects {:?}",
                    app_info.sha256
                )
            }
        } else {
            bail!(
                "app info file {info_path:?} has a valid signature, but the key \
                 used for making the signature is not trusted: \
                 {public_key:?}"
            )
        }
    } else {
        // XX what do i say in other place?
        bail!("signature for app info file {info_path:?} is not valid")
    }
}

/// Carry out the install step of an "install" or "upgrade". For the
/// former, `changelog_output` will be left empty.
pub fn carry_out_install_action(
    binary_path: &Path,
    changelog_output: &str,
    confirm: bool,
    action_verb_in_past_tense: &str,
    program_name: &str,
) -> Result<()> {
    let action = install_executable(&binary_path)?;
    let action_bullet_points = action.show_bullet_points();
    print!("{changelog_output}");
    println!("Will:\n{action_bullet_points}");
    if confirm {
        if !action.is_noop() {
            if !ask_yn("Carry out the above actions?")? {
                bail!("action aborted by user")
            }
        }
    }
    if action.is_noop() {
        println!("There was nothing to do.");
    } else {
        let AppendToShellFileDone {
            provided: _,
            did_change_shell_file,
        } = action.run(())?;
        println!("Successfully {action_verb_in_past_tense} the {program_name} executable.");
        if did_change_shell_file {
            println!("Please open a new shell so that it will find the {program_name} executable.");
        }
    }
    Ok(())
}

/// Currently always upgrades when the remote version is newer. In the
/// FUTURE might want to give explicit version requests, which can be
/// satisfied from the Git history of the binaries repository.
pub struct UpgradeRules {
    pub current_version: GitVersion<SemVersion>,
    /// Applies only if lower version is encountered
    pub force_downgrade: bool,
    /// Applies when the exact same version is encountered
    pub force_reinstall: bool,
    /// Ask for confirmation
    pub confirm: bool,
}

/// Get the repository with the binaries or refresh it, choose the
/// right binary, verify signature on it, install it after possibly
/// checking its app info against the version requirement given in
/// `rules`.
pub fn git_based_upgrade(rules: UpgradeRules) -> Result<()> {
    let VerifiedExecutable {
        binary_path,
        app_info,
        changelog_path,
    } = pull_verified_executable()?;

    let downloaded_version: GitVersion<SemVersion> = app_info.version.parse()?;

    let UpgradeRules {
        current_version,
        force_downgrade,
        force_reinstall,
        confirm,
    } = rules;

    let order = downloaded_version
        .partial_cmp(&current_version)
        .ok_or_else(|| anyhow!("bug, if this happens, Christian doesn't understand PartialOrd"))?;

    enum Action {
        DoNothingBecause(String),
        InstallBecause(String),
    }

    let action = match order {
        Ordering::Less => {
            if force_downgrade {
                Action::InstallBecause(format!("the --force-downgrade option was given"))
            } else {
                Action::DoNothingBecause(format!(
                    "the downloaded version {downloaded_version} is older \
                     than your version {current_version}.\n\
                     Give the --force-downgrade option in case you really want to downgrade \
                     (not recommended)"
                ))
            }
        }
        Ordering::Equal => {
            if force_reinstall {
                Action::InstallBecause(format!("the --force-reinstall option was given"))
            } else {
                Action::DoNothingBecause(format!(
                    "your version {current_version} is already up to date.\n\
                     Give the --force-reinstall option in case you want to re-install"
                ))
            }
        }
        Ordering::Greater => Action::InstallBecause(format!(
            "the downloaded version {downloaded_version} is newer than your version {current_version}"
        )),
    };

    match action {
        Action::DoNothingBecause(msg) => {
            println!("Do nothing because {msg}.");
        }
        Action::InstallBecause(msg) => {
            println!("Installing because {msg}.");

            let changelog_part = {
                let changelog_string = std::fs::read_to_string(&changelog_path)
                    .with_context(|| anyhow!("can't read file {changelog_path:?}"))?;

                let changelog = Changelog::from_str(&changelog_string)?;
                let part =
                    changelog.get_between_versions(true, false, Some(&current_version), None)?;
                let mut out = Vec::new();
                // XX should share the settings with `changelog_command`
                part.display(
                    &ChangelogDisplay {
                        generate_title: true,
                        style: ChangelogDisplayStyle::ReleasesAsSections {
                            print_colon_after_release: true,
                            newest_section_first: false,
                            newest_item_first: false,
                        },
                    },
                    &mut out,
                )?;
                String::from_utf8(out).expect("no utf-8 problems possible")
            };

            let changelog_output = format!(
                "{}{}{}",
                "====Changes coming with the installed version================================\n",
                changelog_part,
                "=============================================================================\n"
            );

            carry_out_install_action(
                &binary_path,
                &changelog_output,
                confirm,
                match order {
                    Ordering::Less => "downgraded",
                    Ordering::Equal => "reinstalled",
                    Ordering::Greater => "upgraded",
                },
                PROGRAM_NAME,
            )?;
        }
    }

    Ok(())
}
