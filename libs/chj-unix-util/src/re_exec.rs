use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

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

/// Only returns when there is an error.
pub fn _re_exec() -> Result<()> {
    let path = std::env::current_exe().context("getting the path to the current executable")?;
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
