use std::{
    fmt::Debug,
    fs::create_dir_all,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Context, Result};

use crate::{
    effect::{bind, Effect, NoOp},
    path_util::AppendToPath,
    utillib::home::{home_dir, HomeError},
};

use super::{
    copy_file::{copy_file, CopiedFile, CopyFile},
    done::Done,
    shell::{AppendToShellFileDone, ShellType},
};

pub fn cargo_bin_dir() -> Result<PathBuf, &'static HomeError> {
    home_dir().map(|path| path.append(".cargo").append("bin"))
}

/// Returns action to copy `path` to `~/.cargo/bin/`, and the path to
/// that latter directory.
pub fn copy_to_cargo_bin_dir<R: Debug>(path: &Path) -> Result<(CopyFile<R>, PathBuf)> {
    let file_name = path
        .file_name()
        .ok_or_else(|| anyhow!("missing file name in path {path:?}"))?;
    let cargo_bin_dir = cargo_bin_dir()?;
    create_dir_all(&cargo_bin_dir).with_context(|| {
        anyhow!("creating directory {cargo_bin_dir:?} or if necessary parent directories")
    })?;
    let target_path = (&cargo_bin_dir).append(file_name);

    let action = copy_file(path, &target_path);
    Ok((action, cargo_bin_dir))
}

/// Copy executable from `path` to `~/.cargo/bin/`, and add the latter
/// path to the shell startup file of the currently running shell, if
/// not already part of the current `PATH`.
pub fn install_executable(
    path: &Path,
) -> Result<Box<dyn Effect<Requires = (), Provides = AppendToShellFileDone<CopiedFile<()>>>>> {
    let shell_type = ShellType::from_env()?;
    let (action1, cargo_bin_dir) = copy_to_cargo_bin_dir(path)?;

    let action2: Box<
        dyn Effect<Requires = CopiedFile<()>, Provides = AppendToShellFileDone<CopiedFile<()>>>,
    > = {
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
            NoOp::passing(
                |provided: CopiedFile<()>| -> AppendToShellFileDone<CopiedFile<()>> {
                    AppendToShellFileDone {
                        provided,
                        done: Done::nothing(),
                    }
                },
                format!(
                    "not changing your shell config file because \
                     your PATH env variable already contains the path {cargo_bin_dir:?}"
                )
                .into(),
            )
        } else {
            Box::new(shell_type.add_to_path_in_init_file(&cargo_bin_dir)?)
        }
    };

    Ok(bind(Box::new(action1), action2))
}
