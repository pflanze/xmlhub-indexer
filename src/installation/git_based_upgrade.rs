//! Upgrading an executable by (cloning and) pulling from a Git
//! repository containing signed binaries.

use std::path::PathBuf;

use anyhow::{anyhow, bail, Context, Result};

use crate::{
    const_util::file_name,
    git::git,
    path_util::AppendToPath,
    sha256::sha256sum,
    xmlhub_indexer_defaults::{BINARIES_CHECKOUT, XMLHUB_INDEXER_BINARY_FILE},
};

use super::{
    app_info::AppInfo,
    app_signature::{AppSignature, SaveLoadKeyFile},
    binaries_repo::BinariesRepoSection,
    defaults::create_installation_state_dir,
    install::install_executable,
    trusted_keys::get_trusted_key,
};

// Todo: change to git remote update and reset, so that trimming the
// upstream repository every now and then would be possible?
pub fn pull_verified_executable() -> Result<PathBuf> {
    let installation_state_dir = create_installation_state_dir()?;

    let binaries_repo_name = "xmlhub-indexer-binaries";

    let binaries_checkout = BINARIES_CHECKOUT
        .replace_working_dir_path(installation_state_dir.append(binaries_repo_name));

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
        .append(file_name(XMLHUB_INDEXER_BINARY_FILE));
    let (info, info_path, info_bytes) = AppInfo::load_for_app_path(&binary_path)?;
    let sig = AppSignature::load_from_base(&info_path)?;
    let (is_valid, public_key) = sig.verify(&info_bytes)?;
    if is_valid {
        if let Some(trusted_key) = get_trusted_key(&public_key) {
            println!(
                "Good info signature made with {trusted_key} on {}",
                sig.metadata.birth
            );
            let actual_hash = sha256sum(&binary_path).with_context(|| anyhow!(""))?;
            if actual_hash == info.sha256 {
                println!("App file hash is valid.");
                Ok(binary_path)
            } else {
                bail!(
                    "invalid file hash: the file {binary_path:?} hashes to {actual_hash:?}, \
                     but its signed info file expects {:?}",
                    info.sha256
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

/// Get the repository with the binaries or refresh it, choose the
/// right binary, verify signature on it, install it.
pub fn git_based_upgrade() -> Result<()> {
    let binary_path = pull_verified_executable()?;
    let done = install_executable(&binary_path)?;
    println!("Upgraded executable:\n\n{done}");
    Ok(())
}
