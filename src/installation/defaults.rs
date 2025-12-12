//! General settings for the installation/upgrade system.

use std::{
    fmt::Display,
    fs::create_dir_all,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Context, Result};
use cj_path_util::path_util::AppendToPath;
use lazy_static::lazy_static;

use crate::utillib::home::home_dir;

/// Relative path to directory from $HOME in which to keep state for
/// the installation, e.g. keys, or a clone of the binaries
/// repository. Probably directly in $HOME, i.e. without slashes.
const INSTALLATION_STATE_DIR: &str = ".xmlhub";

/// Representation of a directory below $HOME in which to keep state
/// for the installation, e.g. keys, a clone of the binaries
/// repository, etc. The full folder structure of that folder should
/// be represented via this type. Method calls to particular
/// subfolders creates these folder(s) if necessary.
pub struct GlobalAppStateDir {
    base_dir: PathBuf,
}

fn create_dir_all_with_context(path: &Path) -> Result<()> {
    // XX private permissions?
    create_dir_all(path).with_context(|| {
        anyhow!("creating directory {path:?} or if necessary parent directories")
    })?;
    Ok(())
}

// Mis-use thiserror just to side-step the problem with &anyhow::Error
// not being OK to use.
#[derive(thiserror::Error, Debug)]
#[error("{message}")]
pub struct GlobalError {
    pub message: String,
}

// impl<T: Display> From<T> for GlobalError {
//     fn from(value: T) -> Self {
//         Self{ message: value.to_string() }
//     }
// }

impl GlobalError {
    pub fn new<T: Display>(val: T) -> Self {
        GlobalError {
            message: val.to_string(),
        }
    }
}

impl GlobalAppStateDir {
    /// Retrieves the $HOME value and creates the main subdir if
    /// necessary.
    pub fn new() -> Result<Self, GlobalError> {
        let home = home_dir().map_err(GlobalError::new)?;
        let base_dir = home.append(INSTALLATION_STATE_DIR);
        create_dir_all_with_context(&base_dir).map_err(GlobalError::new)?;
        Ok(Self { base_dir })
    }

    fn subdir(&self, dir_name: &str) -> Result<PathBuf> {
        let dir = (&self.base_dir).append(dir_name);
        create_dir_all_with_context(&dir)?;
        Ok(dir)
    }

    /// Dir for cloning repositories to (e.g. xmlhub-indexer-binaries)
    pub fn clones_base(&self) -> Result<PathBuf> {
        self.subdir("clones")
    }

    /// Dir for storing info on upgrades that were executed
    pub fn upgrades_log_base(&self) -> Result<PathBuf> {
        self.subdir("upgrades-log")
    }

    /// Dir for storing doc files for showing to the user. Use subdir
    /// by program version, to keep old versions of the docs, for
    /// potentially the user's benefit.
    pub fn docs_base(&self, program_version: &str) -> Result<PathBuf> {
        let dir = self.subdir("docs")?.append(program_version);
        create_dir_all_with_context(&dir)?;
        Ok(dir)
    }
}

lazy_static! {
    static ref GLOBAL_APP_STATE_DIR: Result<GlobalAppStateDir, GlobalError> =
        GlobalAppStateDir::new();
}

pub fn global_app_state_dir() -> Result<&'static GlobalAppStateDir, &'static GlobalError> {
    (*GLOBAL_APP_STATE_DIR).as_ref()
}
