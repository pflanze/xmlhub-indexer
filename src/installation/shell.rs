use std::{
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, Context, Result};

use crate::{path_util::AppendToPath, utillib::home::home_dir};

use super::done::Done;

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum ShellType {
    Bash,
    Csh,
}

impl ShellType {
    pub fn from_env() -> Result<Self> {
        let shell_path: PathBuf = std::env::var("SHELL")
            .context("reading SHELL environment variable")?
            .into();
        let shell_name = shell_path.file_name()
            .ok_or_else(|| anyhow!("SHELL environment variable contains path {shell_path:?} which is missing a file name"))?;
        match shell_name.to_string_lossy().as_ref() {
            "bash" => Ok(Self::Bash),
            "csh" => Ok(Self::Csh),
            _ => bail!("don't know shell {shell_name:?}"),
        }
    }

    pub fn init_file_name(self) -> &'static str {
        match self {
            Self::Bash => ".bashrc",
            Self::Csh => ".cshenv",
        }
    }

    pub fn init_file_path(self) -> Result<PathBuf> {
        Ok(home_dir()?.append(self.init_file_name()))
    }

    /// Add `dir_path` to `PATH` env var. Go the way with the file
    /// that is re-initialized on every shell open, to avoid having to
    /// re-login.
    pub fn add_to_path_in_init_file(self, dir_path: &Path) -> Result<Done> {
        if !dir_path.is_dir() {
            bail!("path does not point to a directory: {dir_path:?}")
        }

        let dir_path_string = dir_path.to_str().with_context(|| {
            anyhow!(
                "path is not representable in unicode, thus can't be put \
                 into shell startup file: {dir_path:?}"
            )
        })?;
        let shell_init_path = self.init_file_path()?;
        (|| -> Result<()> {
            let mut out = File::options()
                .append(true)
                .create(true)
                .open(&shell_init_path)?;
            writeln!(&mut out, "\nPATH=\"{}:$PATH\"", dir_path_string)?;
            out.flush()?;
            Ok(())
        })()
        .with_context(|| anyhow!("writing to file: {shell_init_path:?}"))?;
        Ok(
            format!("added code to {shell_init_path:?} to add path {dir_path_string:?} to PATH")
                .into(),
        )
    }
}
