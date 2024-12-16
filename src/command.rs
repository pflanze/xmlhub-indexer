//! A more convenient interface than std::process::* for running
//! external programs.

use std::{
    ffi::OsStr,
    fmt::Debug,
    path::Path,
    process::{Child, Command, ExitStatus},
};

use anyhow::{anyhow, bail, Context, Result};

fn cmd_args<P: AsRef<OsStr> + Debug, A: AsRef<OsStr> + Debug>(
    cmd: P,
    arguments: &[A],
) -> Vec<String> {
    let cmd_osstr: &OsStr = cmd.as_ref();
    let mut vec = vec![cmd_osstr.to_string_lossy().to_string()];
    for v in arguments {
        let v_osstr: &OsStr = v.as_ref();
        vec.push(v_osstr.to_string_lossy().to_string());
    }
    vec
}

fn check_exitstatus(
    exitstatus: &ExitStatus,
    acceptable_status_codes: &[i32],
    get_cmd_args_dir: &dyn Fn() -> (Vec<String>, String),
) -> Result<bool> {
    if let Some(code) = exitstatus.code() {
        if acceptable_status_codes.contains(&code) {
            Ok(code == 0)
        } else {
            let (cmd_args, in_dir) = get_cmd_args_dir();
            bail!(
                "running {cmd_args:?} in directory {in_dir:?}: \
                 command exited with code {code}",
            )
        }
    } else {
        let (cmd_args, in_dir) = get_cmd_args_dir();
        bail!(
            "running {cmd_args:?} in directory {in_dir:?}: command exited via signal, \
             or other problem",
        )
    }
}

/// Run `cmd` with `arguments` and the env overridden with the
/// key-value pairs in `set_env`, return a `Child` handle (does *not*
/// wait for its completion).
pub fn spawn<P: AsRef<OsStr> + Debug, A: AsRef<OsStr> + Debug>(
    in_directory: &Path,
    cmd: P,
    arguments: &[A],
    set_env: &[(&str, &str)],
) -> Result<Child> {
    let get_cmd_args_dir = || {
        (
            cmd_args(&cmd, arguments),
            in_directory.to_string_lossy().to_string(),
        )
    };
    let mut c1 = Command::new(&cmd);
    c1.args(arguments).current_dir(in_directory);
    for (k, v) in set_env {
        c1.env(k, v);
    }
    c1.spawn().with_context(|| {
        let (cmd_args, in_dir) = get_cmd_args_dir();
        anyhow!("running {cmd_args:?} in directory {in_dir:?}",)
    })
}

/// Run `cmd` with `arguments` and the env overridden with the
/// key-value pairs in `set_env`, wait for its completion. Returns an
/// error if cmd exited with a code that is not in
/// `acceptable_status_codes`.  Returns true when 0 is in
/// acceptable_status_codes and cmd exited with status 0, false for
/// other accepted status codes.
pub fn run<P: AsRef<OsStr> + Debug, A: AsRef<OsStr> + Debug>(
    in_directory: &Path,
    cmd: P,
    arguments: &[A],
    set_env: &[(&str, &str)],
    acceptable_status_codes: &[i32],
) -> Result<bool> {
    let get_cmd_args_dir = || {
        (
            cmd_args(&cmd, arguments),
            in_directory.to_string_lossy().to_string(),
        )
    };
    let mut c = spawn(in_directory, &cmd, arguments, set_env)?;
    let exitstatus = c.wait().with_context(|| {
        let (cmd_args, in_dir) = get_cmd_args_dir();
        anyhow!("running {cmd_args:?} in directory {in_dir:?}",)
    })?;
    check_exitstatus(&exitstatus, acceptable_status_codes, &get_cmd_args_dir)
}

/// Same as `run` but captures and returns stdout.
pub fn run_stdout<P: AsRef<OsStr> + Debug, A: AsRef<OsStr> + Debug>(
    in_directory: &Path,
    cmd: P,
    arguments: &[A],
    set_env: &[(&str, &str)],
    acceptable_status_codes: &[i32],
) -> Result<Vec<u8>> {
    let get_cmd_args_dir = || {
        (
            cmd_args(&cmd, arguments),
            in_directory.to_string_lossy().to_string(),
        )
    };
    // Can't use spawn here, right? XX refactor into yet another
    // sub-function before calling spawn?
    let mut c1 = Command::new(&cmd);
    c1.args(arguments).current_dir(in_directory);
    for (k, v) in set_env {
        c1.env(k, v);
    }
    let c = c1.output().with_context(|| {
        let (cmd_args, in_dir) = get_cmd_args_dir();
        anyhow!("running {cmd_args:?} in directory {in_dir:?}",)
    })?;
    check_exitstatus(&c.status, acceptable_status_codes, &get_cmd_args_dir)?;
    Ok(c.stdout)
}
