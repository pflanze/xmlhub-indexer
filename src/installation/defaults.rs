//! General settings for the installation/upgrade system.

use std::{fs::create_dir_all, path::PathBuf};

use anyhow::{anyhow, Context, Result};

use crate::{path_util::AppendToPath, utillib::home::home_dir};

/// Relative path to directory from $HOME in which to keep state for
/// the installation, e.g. keys, or a clone of the binaries
/// repository. Probably directly in $HOME, i.e. without slashes.
const INSTALLATION_STATE_DIR: &str = ".xmlhub";

/// Full path to directory from $HOME in which to keep state for the
/// installation, e.g. keys, or a clone of the binaries
/// repository. Creates it if necessary.
pub fn create_installation_state_dir() -> Result<PathBuf> {
    let home = home_dir()?;
    let dir = home.append(INSTALLATION_STATE_DIR);
    // XX private permissions?
    create_dir_all(&dir).with_context(|| {
        anyhow!("creating directory {dir:?} or if necessary parent directories")
    })?;
    Ok(dir)
}
