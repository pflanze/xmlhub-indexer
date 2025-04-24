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
    copy_file::{copy_file, CopiedFile},
    shell::{AppendToShellFileDone, ShellType},
};

pub fn cargo_bin_dir() -> Result<PathBuf, &'static HomeError> {
    home_dir().map(|path| path.append(".cargo").append("bin"))
}

/// Returns action to copy `path` to `~/.cargo/bin/`, and the path to
/// that latter directory.
pub fn copy_to_cargo_bin_dir<R: Debug + 'static>(
    path: &Path,
) -> Result<(
    Box<dyn Effect<Requires = R, Provides = CopiedFile<R>>>,
    PathBuf,
)> {
    let file_name = path
        .file_name()
        .ok_or_else(|| anyhow!("missing file name in path {path:?}"))?;

    let cargo_bin_dir = cargo_bin_dir()?;

    {
        let path_parent = path
            .parent()
            .ok_or_else(|| anyhow!("missing parent dir in executable path {path:?}"))?;
        let path_parent = path_parent
            .canonicalize()
            .with_context(|| anyhow!("canonicalizing executable path {path_parent:?}"))?;

        let cargo_bin_dir = cargo_bin_dir
            .canonicalize()
            .with_context(|| anyhow!("canonicalizing cargo bin dir {cargo_bin_dir:?}"))?;

        if path_parent == cargo_bin_dir {
            return Ok((
                NoOp::passing(
                    |provided: R| CopiedFile {
                        provided,
                        replaced: false,
                    },
                    "this executable is already in the installed location".into(),
                ),
                cargo_bin_dir,
            ));
        }
    }
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
                        did_change_shell_file: false,
                    }
                },
                format!(
                    "not changing your shell config file because \
                     your PATH env variable already contains the path {cargo_bin_dir:?}"
                )
                .into(),
            )
        } else {
            shell_type.add_to_path_in_init_file(&cargo_bin_dir)?
        }
    };

    Ok(bind(action1, action2))
}
