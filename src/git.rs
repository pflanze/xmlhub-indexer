use std::{
    ffi::OsStr,
    fmt::Debug,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{anyhow, bail, Context, Result};

use crate::command::{run, run_stdout};

#[derive(Debug, Clone)]
pub struct RelPathWithBase {
    base_path: Option<Arc<PathBuf>>,
    rel_path: PathBuf,
}

impl RelPathWithBase {
    pub fn new(base_path: Option<Arc<PathBuf>>, rel_path: PathBuf) -> Self {
        Self {
            base_path,
            rel_path,
        }
    }

    pub fn full_path(&self) -> PathBuf {
        if let Some(path) = &self.base_path {
            let mut path = (**path).to_owned();
            path.push(&self.rel_path);
            path
        } else {
            self.rel_path.clone()
        }
    }

    pub fn extension(&self) -> Option<&OsStr> {
        self.rel_path.extension()
    }

    pub fn rel_path(&self) -> &str {
        self.rel_path
            // XXX what happens on Windows? UTF-16 always needs recoding, no?
            .to_str()
            .expect("always works since created from str")
    }
}

/// Returns true when git exited with code 0, false if 1; returns an
/// error for other exit codes or errors.
pub fn git<S: AsRef<OsStr> + Debug>(base_path: &Path, arguments: &[S]) -> Result<bool> {
    run(base_path, "git", arguments, &[("PAGER", "")], &[0, 1])
}

/// Only succeeds if Git exited with code 0.
pub fn git_stdout<S: AsRef<OsStr> + Debug>(base_path: &Path, arguments: &[S]) -> Result<Vec<u8>> {
    run_stdout(base_path, "git", arguments, &[], &[0])
}

pub fn git_ls_files(base_path: &Path) -> Result<Vec<RelPathWithBase>> {
    let stdout = git_stdout(base_path, &["ls-files", "-z"])?;
    let base_path = Arc::new(base_path.to_owned());
    stdout
        .split(|b| *b == b'\0')
        .map(|bytes| -> Result<_> {
            let rel_path = std::str::from_utf8(bytes)
                .with_context(|| {
                    anyhow!(
                        "decoding git ls-files output as unicode from directory {:?}: {bytes:?}",
                        base_path.to_string_lossy()
                    )
                })?
                .into();
            Ok(RelPathWithBase {
                base_path: Some(base_path.clone()),
                rel_path,
            })
        })
        .collect::<Result<Vec<_>>>()
}

#[derive(Debug)]
pub struct GitStatusItem {
    pub x: char,
    pub y: char,
    /// Could include "->" for symlinks
    pub path: String,
}

pub fn git_status(base_path: &Path) -> Result<Vec<GitStatusItem>> {
    let stdout = git_stdout(base_path, &["status", "-z"])?;
    stdout
        .split(|b| *b == b'\0')
        .map(|bytes| -> Result<Option<GitStatusItem>> {
            if bytes.is_empty() {
                return Ok(None);
            }
            let line = std::str::from_utf8(bytes).with_context(|| {
                anyhow!(
                    "decoding git status output as unicode from directory {:?}: {bytes:?}",
                    base_path.to_string_lossy()
                )
            })?;
            let mut cs = line.chars();
            (|| -> Result<Option<GitStatusItem>> {
                let x = cs.next().ok_or_else(|| anyhow!("can't parse c0"))?;
                let y = cs.next().ok_or_else(|| anyhow!("can't parse c1"))?;
                let c2 = cs.next().ok_or_else(|| anyhow!("can't parse c2"))?;
                if c2 != ' ' {
                    bail!("c2 is not space")
                }
                let path: String = cs.collect();
                Ok(Some(GitStatusItem { x, y, path }))
            })()
            .with_context(|| {
                anyhow!(
                    "decoding git status output from directory {:?}: {bytes:?}",
                    base_path.to_string_lossy()
                )
            })
        })
        .filter(|v| match v {
            Ok(None) => false,
            _ => true,
        })
        .map(|r| r.map(|v| v.unwrap()))
        .collect::<Result<Vec<_>>>()
}
