use std::io::stderr;
use std::path::PathBuf;
use std::process::Command;
use std::{ffi::OsString, os::unix::process::CommandExt};

use anyhow::{anyhow, Context, Result};

pub fn re_exec_with_executable(executable_path: PathBuf) -> std::io::Error {
    let mut args = std::env::args_os();
    let arg0 = args.next();
    let mut cmd = Command::new(&executable_path);
    cmd.args(args);
    if let Some(a0) = arg0 {
        cmd.arg0(a0);
    }
    cmd.exec()
}

pub fn current_exe() -> Result<PathBuf> {
    let path = std::env::current_exe().context("getting the path to the current executable")?;
    // On Linux, this gets " (deleted)" appended when the binary was
    // replaced. Undo that, sigh.
    let deleted_str = " (deleted)";
    let s = path.to_string_lossy();
    if s.ends_with(deleted_str) {
        let os = path.as_os_str().to_owned();
        let mut bs = os.into_encoded_bytes();
        if bs.ends_with(deleted_str.as_bytes()) {
            bs.truncate(bs.len() - deleted_str.as_bytes().len());
        } else {
            use std::io::Write;
            _ = writeln!(
                &mut stderr(),
                "can't find the bytes after a first match, in {path:?}"
            );
            return Ok(path);
        }
        let os = unsafe { OsString::from_encoded_bytes_unchecked(bs) };
        Ok(os.into())
    } else {
        Ok(path)
    }
}

/// Only returns when there is an error.
pub fn _re_exec() -> Result<()> {
    let path = current_exe()?;
    Err(re_exec_with_executable(path.clone()))
        .with_context(|| anyhow!("executing the binary {path:?}"))
}

/// Only returns when there is an error.
pub fn re_exec() -> anyhow::Error {
    match _re_exec() {
        Ok(()) => unreachable!(),
        Err(e) => e,
    }
}
