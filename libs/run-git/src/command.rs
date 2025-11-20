//! A more convenient interface than std::process::* for running
//! external programs.

use std::{
    borrow::Cow,
    ffi::OsStr,
    fmt::{Debug, Display},
    ops::Deref,
    path::Path,
    process::{Child, Command, ExitStatus, Output, Stdio},
};

use anyhow::{anyhow, bail, Context, Result};

fn lossy_string(v: &[u8]) -> Cow<'_, str> {
    // XX different on Windows?
    String::from_utf8_lossy(v)
}

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

fn check_exitstatus(exitstatus: &ExitStatus, acceptable_status_codes: &[i32]) -> Result<bool> {
    if let Some(code) = exitstatus.code() {
        if acceptable_status_codes.contains(&code) {
            Ok(code == 0)
        } else {
            bail!("command exited with code {code}",)
        }
    } else {
        bail!("command exited via signal, or other problem",)
    }
}

/// Which outputs and how they should be captured. Can't clone, the
/// contained handles are not clonable; use `available` before
/// consuming this if you need to retain the knowledge about available
/// captures.
#[derive(Debug)]
pub struct Capturing {
    stdout: Option<Stdio>,
    stderr: Option<Stdio>,
}

impl Capturing {
    pub fn none() -> Self {
        Self {
            stdout: None,
            stderr: None,
        }
    }
    pub fn stdout() -> Self {
        Self {
            stdout: Some(Stdio::piped()),
            stderr: None,
        }
    }
    pub fn stderr() -> Self {
        Self {
            stdout: None,
            stderr: Some(Stdio::piped()),
        }
    }
    pub fn both() -> Self {
        Self {
            stdout: Some(Stdio::piped()),
            stderr: Some(Stdio::piped()),
        }
    }
    pub fn available(&self) -> AvailableCaptures {
        match self {
            Self {
                stdout: None,
                stderr: None,
            } => AvailableCaptures::None,
            Self {
                stdout: Some(_),
                stderr: None,
            } => AvailableCaptures::Stdout,
            Self {
                stdout: None,
                stderr: Some(_),
            } => AvailableCaptures::Stderr,
            Self {
                stdout: Some(_),
                stderr: Some(_),
            } => AvailableCaptures::Both,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum AvailableCaptures {
    Stdout,
    Stderr,
    Both,
    None,
}

impl AvailableCaptures {
    pub fn from_output(output: &Output) -> Self {
        match (output.stdout.is_empty(), output.stderr.is_empty()) {
            (true, true) => Self::None,
            (true, false) => Self::Stderr,
            (false, true) => Self::Stdout,
            (false, false) => Self::Both,
        }
    }
}

/// Wrapper around `Output` that dereferences to it, but also offers a
/// `truthy` field, and implements `Display` to show a multi-line message
/// with both process status and its outputs.
#[derive(Debug)]
pub struct Outputs<'t> {
    /// Whether process exited with status 0 (or maybe in the future a
    /// set of status codes that represent success)
    pub truthy: bool,
    pub available_captures: AvailableCaptures,
    pub output: Output,
    /// Prefixed to `output` lines, "\t" by default
    pub indent: &'t str,
}

impl<'t> Deref for Outputs<'t> {
    type Target = Output;

    fn deref(&self) -> &Self::Target {
        &self.output
    }
}

const ONLY_SHOW_NON_EMPTY_CAPTURES: bool = true;

fn display_output(
    output: &Output,
    available_captures: AvailableCaptures,
    f: &mut std::fmt::Formatter<'_>,
    indent: &str,
) -> std::fmt::Result {
    let captures = if ONLY_SHOW_NON_EMPTY_CAPTURES {
        AvailableCaptures::from_output(output)
    } else {
        available_captures
    };
    match captures {
        AvailableCaptures::Stdout => f.write_fmt(format_args!(
            "{},\n{indent}stdout: {}",
            output.status,
            lossy_string(&output.stdout),
        )),
        AvailableCaptures::Stderr => f.write_fmt(format_args!(
            "{},\n{indent}stderr: {}",
            output.status,
            lossy_string(&output.stderr)
        )),
        AvailableCaptures::Both => f.write_fmt(format_args!(
            "{},\n{indent}stdout: {}\n{indent}stderr: {}",
            output.status,
            lossy_string(&output.stdout),
            lossy_string(&output.stderr)
        )),
        AvailableCaptures::None => f.write_fmt(format_args!("{}", output.status,)),
    }
}

impl<'t> Display for Outputs<'t> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        display_output(&self.output, self.available_captures, f, self.indent)
    }
}

// And a temporary helper to re-use `display_output` when truthy is
// not available
struct DisplayOutput<'t> {
    available_captures: AvailableCaptures,
    output: &'t Output,
    indent: &'t str,
}

impl<'t> Display for DisplayOutput<'t> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        display_output(&self.output, self.available_captures, f, self.indent)
    }
}

fn check_exitstatus_context<'t, P: AsRef<OsStr> + Debug, A: AsRef<OsStr> + Debug>(
    in_directory: &'t Path,
    cmd: &'t P,
    arguments: &'t [A],
    available_captures: AvailableCaptures,
    output: &'t Output,
) -> impl Fn() -> anyhow::Error + 't {
    move || {
        let (cmd_args, in_dir, output) = (
            cmd_args(cmd, arguments),
            in_directory.to_string_lossy().to_string(),
            DisplayOutput {
                available_captures,
                output,
                indent: "\t",
            },
        );
        anyhow!("running {cmd_args:?} in directory {in_dir:?}, {output}")
    }
}

pub fn command_with_settings<D: AsRef<Path>, P: AsRef<OsStr> + Debug, A: AsRef<OsStr> + Debug>(
    in_directory: D,
    cmd: P,
    arguments: &[A],
    set_env: &[(&str, &str)],
    captures: Capturing,
) -> Command {
    let mut c = Command::new(&cmd);
    c.args(arguments).current_dir(in_directory);
    for (k, v) in set_env {
        c.env(k, v);
    }
    c.stdout(if let Some(cap) = captures.stdout {
        cap
    } else {
        Stdio::inherit()
    });
    c.stderr(if let Some(cap) = captures.stderr {
        cap
    } else {
        Stdio::inherit()
    });
    c
}

/// Run `cmd` with `arguments` and the env overridden with the
/// key-value pairs in `set_env`, return a `Child` handle (does *not*
/// wait for its completion). If you don't want to capture outputs,
/// pass `Captures::none()` to `captures`. Note: if you just want to
/// run a process in the background and are not actually reading from
/// the filehandles in `Child`, then you definitely want to set this
/// to `Captures::none()`, because otherwise the outputs will go
/// nowhere, or even block the child process after filling the pipe
/// buffer.
pub fn spawn<D: AsRef<Path>, P: AsRef<OsStr> + Debug, A: AsRef<OsStr> + Debug>(
    in_directory: D,
    cmd: P,
    arguments: &[A],
    set_env: &[(&str, &str)],
    captures: Capturing,
) -> Result<Child> {
    let mut c = command_with_settings(in_directory.as_ref(), &cmd, arguments, set_env, captures);
    c.spawn().with_context(|| {
        let (cmd_args, in_dir) = (
            cmd_args(&cmd, arguments),
            in_directory.as_ref().to_string_lossy().to_string(),
        );
        anyhow!("running {cmd_args:?} in directory {in_dir:?}",)
    })
}

/// Run `cmd` with `arguments` and the env overridden with the
/// key-value pairs in `set_env`, wait for its completion. Returns an
/// error if cmd exited with a code that is not in
/// `acceptable_status_codes`.  Returns true when 0 is in
/// acceptable_status_codes and cmd exited with status 0, false for
/// other accepted status codes. `silencing` specifies captures that
/// should be done, which are dropped unless there's an error.
pub fn run<D: AsRef<Path>, P: AsRef<OsStr> + Debug, A: AsRef<OsStr> + Debug>(
    in_directory: D,
    cmd: P,
    arguments: &[A],
    set_env: &[(&str, &str)],
    acceptable_status_codes: &[i32],
    silencing: Capturing,
) -> Result<bool> {
    let get_cmd_args_dir = || {
        (
            cmd_args(&cmd, arguments),
            in_directory.as_ref().to_string_lossy().to_string(),
        )
    };
    let available_captures = silencing.available();
    let output = run_output(in_directory.as_ref(), &cmd, arguments, set_env, silencing)?;
    let exitstatus = output.status;
    check_exitstatus(&exitstatus, acceptable_status_codes).with_context(|| {
        let (cmd_args, in_dir) = get_cmd_args_dir();
        let outputs = Outputs {
            truthy: false,
            available_captures,
            output,
            indent: "", // XX
        };
        anyhow!("running {cmd_args:?} in directory {in_dir:?}: {outputs}")
    })
}

pub fn run_output<D: AsRef<Path>, P: AsRef<OsStr> + Debug, A: AsRef<OsStr> + Debug>(
    in_directory: D,
    cmd: P,
    arguments: &[A],
    set_env: &[(&str, &str)],
    captures: Capturing,
) -> Result<Output> {
    let mut c = command_with_settings(in_directory.as_ref(), &cmd, arguments, set_env, captures);
    c.output().with_context(|| {
        let (cmd_args, in_dir) = (
            cmd_args(&cmd, arguments),
            in_directory.as_ref().to_string_lossy().to_string(),
        );
        anyhow!("running {cmd_args:?} in directory {in_dir:?}",)
    })
}

/// Same as `run` but captures outputs, returning (exited_0, stdout,
/// stderr)
pub fn run_outputs<D: AsRef<Path>, P: AsRef<OsStr> + Debug, A: AsRef<OsStr> + Debug>(
    in_directory: D,
    cmd: P,
    arguments: &[A],
    set_env: &[(&str, &str)],
    acceptable_status_codes: &[i32],
) -> Result<Outputs<'static>> {
    let captures = Capturing::both();
    let available_captures = captures.available();
    let output = run_output(in_directory.as_ref(), &cmd, arguments, set_env, captures)?;
    let truthy = check_exitstatus(&output.status, acceptable_status_codes).with_context(|| {
        let (cmd_args, in_dir, output) = (
            cmd_args(&cmd, arguments),
            in_directory.as_ref().to_string_lossy().to_string(),
            DisplayOutput {
                available_captures,
                output: &output,
                indent: "\t",
            },
        );
        anyhow!("running {cmd_args:?} in directory {in_dir:?}, {output}")
    })?;
    Ok(Outputs {
        output,
        truthy,
        available_captures,
        indent: "\t\t",
    })
}

/// Same as `run` but captures and returns stdout.
pub fn run_stdout<D: AsRef<Path>, P: AsRef<OsStr> + Debug, A: AsRef<OsStr> + Debug>(
    in_directory: D,
    cmd: P,
    arguments: &[A],
    set_env: &[(&str, &str)],
    acceptable_status_codes: &[i32],
) -> Result<Outputs<'static>> {
    let captures = Capturing::stdout();
    let available_captures = captures.available();
    let output = run_output(in_directory.as_ref(), &cmd, arguments, set_env, captures)?;
    let truthy = check_exitstatus(&output.status, acceptable_status_codes).with_context(|| {
        let (cmd_args, in_dir, output) = (
            cmd_args(&cmd, arguments),
            in_directory.as_ref().to_string_lossy().to_string(),
            DisplayOutput {
                available_captures,
                output: &output,
                indent: "\n",
            },
        );
        anyhow!("running {cmd_args:?} in directory {in_dir:?}, {output}")
    })?;
    Ok(Outputs {
        output,
        truthy,
        available_captures,
        indent: "\t\t",
    })
}

/// Same as `run` but captures and returns stderr.
pub fn run_stderr<P: AsRef<OsStr> + Debug, A: AsRef<OsStr> + Debug>(
    in_directory: &Path,
    cmd: P,
    arguments: &[A],
    set_env: &[(&str, &str)],
    acceptable_status_codes: &[i32],
) -> Result<Outputs<'static>> {
    let captures = Capturing::stderr();
    let available_captures = captures.available();
    let output = run_output(in_directory, &cmd, arguments, set_env, captures)?;
    let truthy = check_exitstatus(&output.status, acceptable_status_codes).with_context(
        check_exitstatus_context(in_directory, &cmd, arguments, available_captures, &output),
    )?;
    Ok(Outputs {
        output,
        truthy,
        available_captures,
        indent: "\t\t",
    })
}

/// Same as `run_stdout` but returns stdout as a utf-8 decoded string.
pub fn run_stdout_string<D: AsRef<Path>, P: AsRef<OsStr> + Debug, A: AsRef<OsStr> + Debug>(
    in_directory: D,
    cmd: P,
    arguments: &[A],
    set_env: &[(&str, &str)],
    acceptable_status_codes: &[i32],
    trim_ending_newline: bool,
) -> Result<String> {
    let outputs = run_stdout(
        in_directory,
        cmd,
        arguments,
        set_env,
        acceptable_status_codes,
    )?;
    let mut stdout = String::from_utf8(outputs.output.stdout)?;
    if trim_ending_newline {
        let end = "\n";
        if stdout.ends_with(end) {
            stdout = stdout[0..stdout.len() - end.len()].into();
        }
    }
    Ok(stdout)
}
