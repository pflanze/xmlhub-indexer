use std::{
    fmt::Debug,
    fs::File,
    io::Write,
    marker::PhantomData,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, Context, Result};

use crate::{effect::Effect, path_util::AppendToPath, utillib::home::home_dir};

use super::done::Done;

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum ShellType {
    Bash,
    Csh,
    Zsh,
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
            "zsh" => Ok(Self::Zsh),
            _ => bail!("don't know shell {shell_name:?}"),
        }
    }

    pub fn init_file_name(self) -> &'static str {
        match self {
            Self::Bash => ".bashrc",
            Self::Csh => ".cshenv",
            Self::Zsh => ".zshenv",
        }
    }

    pub fn init_file_path(self) -> Result<PathBuf> {
        Ok(home_dir()?.append(self.init_file_name()))
    }

    /// Return action to add `dir_path` to `PATH` env var. Go the way
    /// with the file that is re-initialized on every shell open, to
    /// avoid having to re-login (XX todo: properly?).
    pub fn add_to_path_in_init_file<R: Debug>(
        self,
        dir_path: &Path,
    ) -> Result<AppendToShellFile<R>> {
        if !dir_path.is_dir() {
            bail!("path does not point to a directory: {dir_path:?}")
        }

        let dir_path_string = dir_path.to_str().with_context(|| {
            anyhow!(
                "path is not representable in unicode, thus can't be put \
                 into shell startup file: {dir_path:?}"
            )
        })?;

        let file_path = self.init_file_path()?;
        let code_to_append = format!("\nPATH=\"{}:$PATH\"", dir_path_string);
        let to_be_done = Done::from(format!(
            "add code to {file_path:?} to add the path {dir_path_string:?} to \
             the PATH environment variable"
        ));
        Ok(AppendToShellFile {
            phantom: Default::default(),
            file_path,
            code_to_append,
            to_be_done,
        })
    }
}

#[derive(Debug)]
pub struct AppendToShellFile<R> {
    phantom: PhantomData<fn() -> R>,
    pub file_path: PathBuf,
    pub code_to_append: String,
    pub to_be_done: Done,
}

#[derive(Debug)]
pub struct AppendToShellFileDone<R> {
    pub provided: R,
    pub done: Done,
}

impl<R: Debug> Effect for AppendToShellFile<R> {
    type Requires = R;

    type Provides = AppendToShellFileDone<R>;

    fn run(self: Box<Self>, provided: R) -> Result<Self::Provides> {
        let Self {
            file_path,
            code_to_append,
            to_be_done,
            phantom: _,
        } = *self;
        (|| -> Result<()> {
            let mut out = File::options().append(true).create(true).open(&file_path)?;
            writeln!(&mut out, "{code_to_append}")?;
            out.flush()?;
            Ok(())
        })()
        .with_context(|| anyhow!("writing to file: {file_path:?}"))?;
        Ok(AppendToShellFileDone {
            provided,
            done: to_be_done,
        })
    }
}
