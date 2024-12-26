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
    run_stdout(base_path, "git", arguments, &[], &[0]).map(|o| o.output.stdout)
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
                        "decoding git ls-files output as unicode from directory {:?}: {:?}",
                        base_path.to_string_lossy(),
                        String::from_utf8_lossy(bytes)
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
    pub target_path: Option<String>,
}

fn parse_git_status_record(line: &str) -> Result<GitStatusItem> {
    let mut cs = line.chars();
    let x = cs.next().ok_or_else(|| anyhow!("can't parse c0"))?;
    let y = cs.next().ok_or_else(|| anyhow!("can't parse c1"))?;
    let c2 = cs.next().ok_or_else(|| anyhow!("can't parse c2"))?;
    if c2 != ' ' {
        bail!("c2 {c2:?} is not space, x={x:?}, y={y:?}")
    }
    let path: String = cs.collect();
    Ok(GitStatusItem {
        x,
        y,
        path,
        target_path: None,
    })
}

pub fn git_status(base_path: &Path) -> Result<Vec<GitStatusItem>> {
    let decode_line = |line_bytes| {
        std::str::from_utf8(line_bytes).with_context(|| {
            anyhow!(
                "decoding git status output as unicode from directory {:?}: {:?}",
                base_path.to_string_lossy(),
                String::from_utf8_lossy(line_bytes)
            )
        })
    };
    let stdout = git_stdout(base_path, &["status", "-z"])?;
    let mut output = Vec::new();
    let mut lines = stdout.split(|b| *b == b'\0');
    while let Some(line_bytes) = lines.next() {
        if line_bytes.is_empty() {
            // Happens if stdout is empty!
            continue;
        }
        let line = decode_line(line_bytes)?;
        let record = parse_git_status_record(&line).with_context(|| {
            anyhow!(
                "decoding git status output from directory {:?}: {:?}",
                base_path.to_string_lossy(),
                String::from_utf8_lossy(line_bytes)
            )
        })?;
        if record.x == 'R' {
            let line_bytes = lines.next().ok_or_else(|| {
                anyhow!(
                    "missing git status target path entry after 'R' \
                     for record {record:?}, \
                     from directory {:?}: {:?}",
                    base_path.to_string_lossy(),
                    String::from_utf8_lossy(line_bytes)
                )
            })?;
            let line2 = decode_line(line_bytes)?;
            output.push(GitStatusItem {
                x: record.x,
                y: record.y,
                path: record.path,
                target_path: Some(line2.into()),
            });
        } else {
            output.push(record);
        }
    }
    Ok(output)
}
