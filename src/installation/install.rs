use std::{
    fs::{copy, create_dir_all, remove_file},
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Context, Result};

use crate::{
    path_util::AppendToPath,
    utillib::home::{home_dir, HomeError},
};

use super::{done::Done, shell::ShellType};

pub fn cargo_bin_dir() -> Result<PathBuf, &'static HomeError> {
    home_dir().map(|path| path.append(".cargo").append("bin"))
}

pub fn copy_file(source_path: &Path, target_path: &Path) -> Result<Done> {
    if target_path.exists() {
        remove_file(target_path)
            .with_context(|| anyhow!("removing existing file {target_path:?}"))?;
    }

    copy(source_path, target_path)
        .with_context(|| anyhow!("copying file from {source_path:?} to {target_path:?}"))?;

    Ok(format!("copied file from {source_path:?} to {target_path:?}").into())
}

/// Returns the path to the bin dir
pub fn copy_to_cargo_bin_dir(path: &Path) -> Result<(Done, PathBuf)> {
    let file_name = path
        .file_name()
        .ok_or_else(|| anyhow!("missing file name in path {path:?}"))?;
    let cargo_bin_dir = cargo_bin_dir()?;
    create_dir_all(&cargo_bin_dir).with_context(|| {
        anyhow!("creating directory {cargo_bin_dir:?} or if necessary parent directories")
    })?;
    let target_path = (&cargo_bin_dir).append(file_name);
    let done = copy_file(path, &target_path)?;
    Ok((done, cargo_bin_dir))
}

/// Copy executable from `path` to `~/.cargo/bin/`, and add the latter
/// path to the shell startup file of the currently running shell, if
/// not already part of the current `PATH`.
pub fn install_executable(path: &Path) -> Result<Done> {
    let shell_type = ShellType::from_env()?;
    let (done1, cargo_bin_dir) = copy_to_cargo_bin_dir(path)?;

    let done2 = {
        // Do we need to add to PATH? Check our current env:
        let path_var = std::env::var("PATH").with_context(|| anyhow!("getting PATH env var"))?;
        let parts: Vec<PathBuf> = path_var
            .split(':')
            .filter_map(|path: &str| -> Option<PathBuf> {
                let path: &Path = path.as_ref();
                match path.canonicalize() {
                    Ok(path) => Some(path),
                    Err(_e) => {
                        // print invalid paths in PATH? not necessary.
                        None
                    }
                }
            })
            .collect();
        if parts.contains(&cargo_bin_dir) {
            Done::nothing()
        } else {
            shell_type.add_to_path_in_init_file(&cargo_bin_dir)?
        }
    };

    Ok(done2.with_previously(done1))
}
