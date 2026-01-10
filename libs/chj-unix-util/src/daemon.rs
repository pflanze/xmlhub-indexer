//! Infrastructure to run / start / stop a service as a daemon process
//! (or process group).

//! See [daemon](../docs/daemon.md) for more info.

use std::{
    fs::{create_dir, remove_file, rename, File},
    io::{stderr, stdout, BufRead, BufReader, ErrorKind, Write},
    num::{NonZeroU32, ParseIntError},
    os::{fd::FromRawFd, unix::prelude::MetadataExt},
    path::{Path, PathBuf},
    str::FromStr,
    sync::{atomic::Ordering, Arc},
    thread::sleep,
    time::Duration,
};

use anyhow::{anyhow, bail, Context};
use chrono::{Local, Utc};
use cj_path_util::path_util::AppendToPath;
use nix::{
    libc::getsid,
    sys::signal::{
        kill,
        Signal::{self, SIGCONT, SIGKILL, SIGSTOP},
    },
    unistd::{close, dup2, pipe, setsid, Pid},
};

use crate::{
    file_lock::{file_lock_nonblocking, FileLockError},
    file_util::open_append,
    polling_signals::{IPCAtomicError, IPCAtomicU64},
    re_exec::re_exec,
    retry::{retry, retry_n},
    unix::easy_fork,
};

#[derive(Debug, Clone, Copy, clap::Subcommand)]
pub enum DaemonMode {
    /// Do not put into background, just run forever in the foreground.
    Run,

    /// Start daemon into background.
    Start,

    /// Stop daemon running in background (does not stop daemons in
    /// `run` mode, only those in `start` mode). This subcommand has
    /// options.
    Stop(StopOpts),

    /// `stop` (ignoring errors) then `start`. This subcommand has
    /// options.
    Restart(StopOpts),

    /// Report if there is a daemon in `start` or `run` mode.
    Status,

    /// Send a STOP signal to (i.e. suspend) the daemon.
    STOP,

    /// Send a CONT signal to (i.e. continue) the daemon.
    CONT,

    /// Send a KILL signal to (i.e. terminate right away) the daemon.
    KILL,
}

const FROM_STR_CASES: &[(&str, DaemonMode)] = {
    const fn opts(force: bool) -> StopOpts {
        StopOpts {
            force,
            wait: false,
            timeout_before_sigkill: 30,
        }
    }
    {
        use DaemonMode::*;
        // reminder to adapt the code below when the enum changes
        match DaemonMode::Run {
            Run => (),
            Start => (),
            Stop(_) => (),
            Restart(_) => (),
            Status => (),
            STOP => (),
            CONT => (),
            KILL => (),
        }
    }

    &[
        ("run", DaemonMode::Run),
        ("start", DaemonMode::Start),
        ("up", DaemonMode::Start),
        ("stop", DaemonMode::Stop(opts(false))),
        ("force-stop", DaemonMode::Stop(opts(true))),
        ("down", DaemonMode::Stop(opts(false))),
        ("force-down", DaemonMode::Stop(opts(true))),
        ("restart", DaemonMode::Restart(opts(false))),
        ("force-restart", DaemonMode::Restart(opts(true))),
        ("status", DaemonMode::Status),
        ("STOP", DaemonMode::STOP),
        ("CONT", DaemonMode::CONT),
        ("KILL", DaemonMode::KILL),
    ]
};

fn errmsg() -> String {
    // Cannot do join() since not using itertools in this crate.
    let mut s = String::from(
        "('start' and 'up', and 'stop' and 'down' and their force variants are \
         aliases; actions with all-uppercase names are sending the signals \
         with the same names): ",
    );
    for (k, _m) in FROM_STR_CASES {
        use std::fmt::Write;
        _ = write!(&mut s, "`{k}`, ");
    }
    s.truncate(s.len() - 2);
    s
}

#[derive(thiserror::Error, Debug)]
#[error("please give one of the strings {}", errmsg())]
pub struct DaemonModeError;

impl FromStr for DaemonMode {
    type Err = DaemonModeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        for (k, v) in FROM_STR_CASES {
            if s == *k {
                return Ok(v.clone());
            }
        }
        Err(DaemonModeError)
    }
}

#[derive(Debug, Clone, clap::Args)]
pub struct DaemonOpts {
    /// If true, write log time stamps in the local time zone.
    /// Default: in UTC.
    #[clap(long)]
    pub local_time: bool,

    /// The maximum size of the 'current.log' file in bytes before it
    /// is renamed and a new one opened.
    #[clap(long, default_value = "10000000")]
    pub max_log_file_size: u64,

    /// The maximum number of numbered log files (i.e. excluding
    /// `current.log`) before the oldest are deleted.  Careful: as
    /// many files are deleted as needed as necessary to get their
    /// count down to the given number (giving 0 it will delete them
    /// all)! None means, no files are ever deleted.
    #[clap(long)]
    pub max_log_files: Option<u32>,
}

pub struct Daemon<F: FnOnce(DaemonStateReader)> {
    pub opts: DaemonOpts,
    /// Where the lock/pid files should be written to (is created if missing).
    pub state_dir: Arc<Path>,
    /// Where the log files should be written to (is created if missing).
    pub log_dir: Arc<Path>,
    /// The code to run; the daemon[XXX is meant to, cleanup]
    /// ends/stops when this function returns. The function should
    /// periodically call `want()` on its argument and stop processing
    /// when it doesn't give `DaemonWant::Up` anymore. XXX it should
    /// also re-exec itself if it is Restart? Or will the library do
    /// that?.
    pub run: F,
}

#[derive(thiserror::Error, Debug)]
pub enum InOutError {
    #[error("IO error: {0}")]
    IOError(#[from] std::io::Error),
    #[error("IO error: {0}")]
    Errno(#[from] nix::errno::Errno),
    #[error("integer parsing error: {0}")]
    ParseIntError(#[from] ParseIntError),
}

#[derive(thiserror::Error, Debug)]
#[error("could not {what} at {path:?}: {error}")]
pub struct PathIOError {
    what: &'static str,
    path: Arc<Path>,
    error: InOutError,
}

#[derive(thiserror::Error, Debug)]
pub enum DaemonError {
    #[error("can't lock file {lock_path:?}: {error}")]
    LockError { lock_path: Arc<Path>, error: String },
    #[error("{context}: IO error: {error}")]
    IoError {
        context: &'static str,
        error: std::io::Error,
    },
    #[error("{context}: {error}")]
    ErrnoError {
        context: &'static str,
        error: nix::errno::Errno,
    },
    #[error("{0}")]
    IPCAtomicError(#[from] IPCAtomicError),
    #[error("{0}")]
    Anyhow(#[from] anyhow::Error),
}

pub struct DaemonResult {
    daemon_state: DaemonStateAccessor,
}

impl DaemonResult {
    pub fn daemon_cleanup(self) {
        let DaemonResult { daemon_state } = self;
        let (want, old_pid) = daemon_state.read();
        // Should not need to change the pid, right?
        let current_sid = unsafe {
            // There's actually no safety issue with getside?
            getsid(0)
        };
        if Some(current_sid) != old_pid {
            eprintln!(
                "warning on stop or restart: our session-id is {current_sid}, but \
                 daemon state has {old_pid:?}. Overwriting it."
            );
        }
        match want {
            DaemonWant::Down => {
                daemon_state.store(DaemonWant::Down, None);
            }
            DaemonWant::Up | DaemonWant::Restart => {
                daemon_state.store(DaemonWant::Up, Some(current_sid));
                // (Ah, the new instance will overwrite daemon_state
                // again, with a new sid.)
                let err = re_exec();
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
    }
}

enum _ExecutionResult {
    /// In the parent process that started the daemon: no value
    Initiator,
    /// In the daemon child: context for handling exiting/restarts
    /// during shutdown. Pass up to the main function, call
    /// `daemon_cleanup`.
    Daemon(DaemonResult),
    /// Daemon-less `Run` result. No need to execute any restarts or
    /// change any daemon state.
    Run,
}

struct Bomb(bool);
impl Drop for Bomb {
    fn drop(&mut self) {
        if self.0 {
            panic!("`ExecutionResult`s need to be passed to their `daemon_cleanup` method");
        }
    }
}

#[must_use]
pub struct ExecutionResult(_ExecutionResult, Bomb);

impl ExecutionResult {
    fn initiator() -> Self {
        Self(_ExecutionResult::Initiator, Bomb(true))
    }

    fn run() -> Self {
        Self(_ExecutionResult::Run, Bomb(true))
    }

    fn daemon(r: DaemonResult) -> Self {
        Self(_ExecutionResult::Daemon(r), Bomb(true))
    }

    /// If need to know if in the daemon, e.g. to only conditionally return to `main`
    pub fn is_daemon(&self) -> bool {
        match &self.0 {
            _ExecutionResult::Initiator => false,
            _ExecutionResult::Daemon(_) => true,
            _ExecutionResult::Run => false,
        }
    }

    /// Call this in the `main` function, after everything in the app
    /// has been cleaned up. If this is in the daemon child, it will
    /// re-exec the daemon binary if this was a restart
    /// action. Otherwise exits, indicating whether this is a daemon
    /// context (same as `is_daemon`).
    pub fn daemon_cleanup(self) -> bool {
        let Self(er, mut bomb) = self;
        bomb.0 = false;
        match er {
            _ExecutionResult::Initiator => false,
            _ExecutionResult::Daemon(daemon_result) => {
                daemon_result.daemon_cleanup();
                true
            }
            _ExecutionResult::Run => false,
        }
    }
}

#[derive(Debug, Clone, Copy, clap::Args)]
pub struct StopOpts {
    /// By default, 'stop' stops the daemon gracefully (signals it the
    /// wish for termination, but the daemon may delay the exit for a
    /// long time or ignore it completely). This stops the daemon via
    /// signals instead, first SIGINT, then SIGKILL.
    #[clap(short, long)]
    pub force: bool,

    /// When doing graceful termination, by default the stop/restart
    /// actions do not wait for the daemon to carry it out. This
    /// changes the behaviour to wait in that case, too.
    #[clap(short, long)]
    pub wait: bool,

    /// The time in seconds after sending SIGINT before sending
    /// SIGKILL
    #[clap(short, long)]
    pub timeout_before_sigkill: u32,
}

#[derive(Debug, Clone)]
pub struct StopReport {
    pub was_pid: Option<i32>,
    pub was_running: bool,
    pub sent_sigint: bool,
    pub sent_sigkill: bool,
    pub crashed: bool,
}

impl<F: FnOnce(DaemonStateReader)> Daemon<F> {
    pub fn create_dirs(&self) -> Result<(), PathIOError> {
        // XX add to file_util, including PathIOError perhaps?
        let create = |path: &Arc<Path>| match create_dir(&path) {
            Ok(()) => Ok(()),
            Err(error) => match error.kind() {
                ErrorKind::AlreadyExists => Ok(()),
                _ => Err(PathIOError {
                    what: "create dir",
                    path: path.clone(),
                    error: error.into(),
                }),
            },
        };
        create(&self.state_dir())?;
        create(&self.log_dir())?;
        Ok(())
    }

    pub fn state_dir(&self) -> Arc<Path> {
        self.state_dir.clone()
    }

    pub fn log_dir(&self) -> Arc<Path> {
        self.log_dir.clone()
    }

    /// Path to a file that is used as a 8-byte mmap file, and for
    /// flock. Protect this file from modification by other
    /// parties--doing so can segfault the app!
    pub fn daemon_state_path(&self) -> Arc<Path> {
        self.state_dir().append("daemon_state.mmap").into()
    }

    /// The same as `daemon_state_path`
    pub fn lock_path(&self) -> Arc<Path> {
        self.daemon_state_path()
    }

    pub fn current_log_path(&self) -> PathBuf {
        self.log_dir().append("current.log")
    }

    fn daemon_state(&self) -> anyhow::Result<DaemonStateAccessor> {
        let daemon_state_path = self.daemon_state_path();
        DaemonStateAccessor::open(daemon_state_path.clone())
            .with_context(|| anyhow!("opening {daemon_state_path:?}"))
    }

    /// Rename the "current.log" file (if present) to "00001.log" or
    /// similar, allocating a new number, and delete old log files if
    /// there are more than configured.
    pub fn rotate_logs(&self) -> anyhow::Result<()> {
        let log_dir = self.log_dir();
        let mut numbered_logfiles = Vec::new();
        for entry in std::fs::read_dir(&log_dir)? {
            let entry = entry?;
            let file_name = entry.file_name();
            if let Some(file_name) = file_name.to_str() {
                if let Some((numstr, _)) = file_name.split_once('.') {
                    if let Ok(num) = usize::from_str(numstr) {
                        numbered_logfiles.push((num, entry.path()));
                    }
                }
            }
        }
        numbered_logfiles.sort_by_key(|(num, _)| *num);
        let last_number = numbered_logfiles.last().map(|(num, _)| *num).unwrap_or(0);
        let new_number = last_number + 1;
        let new_log_path = (&log_dir).append(&format!("{new_number:05}.log"));
        let current_log_path = self.current_log_path();
        match rename(&current_log_path, &new_log_path) {
            Ok(_) => numbered_logfiles.push((new_number, new_log_path)),
            Err(_) => (), // guess there's no file? XX look into what error it is
        };
        let num_numbered_logfiles = numbered_logfiles.len();
        if let Some(max_log_files) = self.opts.max_log_files {
            let max_log_files = usize::try_from(max_log_files).expect("u32 fits in usize");
            if num_numbered_logfiles > max_log_files {
                let delete_n = num_numbered_logfiles - max_log_files;
                for (_, path) in &numbered_logfiles[0..delete_n] {
                    remove_file(path).with_context(|| anyhow!("deleting log file {path:?}"))?;
                    // eprintln!("deleted log file {path:?}"); --ah, can't, no logging output
                }
            }
        }
        Ok(())
    }

    /// Check via flock (sufficient, although the `DaemonState` should
    /// also provide a pid in this case). Slightly costly as it
    /// involves memory allocations and multiple syscalls.
    pub fn is_running(&self) -> Result<bool, anyhow::Error> {
        let lock_path = self.lock_path();
        // The daemon takes an exclusive lock; it's enough and
        // necessary to take a non-exclusive one here, so that
        // multiple testers don't find a lock by accident. XX test
        match file_lock_nonblocking(&lock_path, false) {
            Ok(lock) => {
                // We only get the (non-exclusive) lock as side effect
                // of our approach of testing for it being
                // locked. Drop it right away to minimize the risk for
                // a `start` action failing to get the exclusive lock.
                drop(lock);
                Ok(false)
            }
            Err(e) => match e {
                FileLockError::AlreadyLocked => Ok(true),
                _ => bail!("lock error on {lock_path:?}: {e}"),
            },
        }
    }

    fn _send_signal(&self, pid: i32, signal: Option<Signal>) -> anyhow::Result<bool> {
        let process_group_id: i32 = pid
            .checked_neg()
            .ok_or_else(|| anyhow!("pid {pid} can't be negated"))?;

        match kill(Pid::from_raw(process_group_id), signal) {
            Ok(()) => Ok(true),
            Err(e) => match e {
                nix::errno::Errno::EPERM => {
                    // Can't happen because there's "no way"
                    // that between us checking is_running()
                    // and reading the pid and signalling
                    // another process group would be there
                    // than ours.  XX except, what if a member
                    // of the process group exec's a setuid
                    // binary?
                    panic!(
                        "don't have permission to send signal to \
                                     process group {process_group_id}"
                    )
                }
                nix::errno::Errno::ESRCH => {
                    // Process does not exist
                    Ok(false)
                }
                _ => unreachable!(),
            },
        }
    }

    pub fn send_signal(&self, signal: Option<Signal>) -> anyhow::Result<bool> {
        let daemon_state = self.daemon_state()?;
        let (_old_want, was_pid) = daemon_state.read();
        if let Some(pid) = was_pid {
            retry(|| daemon_state.store_want(DaemonWant::Down));
            self._send_signal(pid, signal)
        } else {
            Ok(false)
        }
    }

    // (Giving up and just using anyhow here)
    fn stop_or_restartstop(
        &self,
        want: DaemonWant,
        opts: StopOpts,
    ) -> Result<StopReport, anyhow::Error> {
        let StopOpts {
            force,
            wait,
            timeout_before_sigkill,
        } = opts;

        let daemon_state = self.daemon_state()?;
        let (_old_want, was_pid) = daemon_state.read();

        let was_running = self.is_running()?;

        // Set the want even if not running: the want might have been
        // Running from before a reboot.
        let (_, old_state) = retry(|| daemon_state.store_want(want));

        let mut sent_sigint = false;
        let mut sent_sigkill = false;
        let mut crashed = false;

        if was_running {
            if force {
                if let Some(pid) = was_pid {
                    if self._send_signal(pid, Some(Signal::SIGINT))? {
                        sent_sigint = true;
                        let num_sleeps = timeout_before_sigkill * 5;
                        for _ in 0..num_sleeps {
                            sleep(Duration::from_millis(200));
                            if !self._send_signal(pid, None)? {
                                break;
                            }
                        }
                        self._send_signal(pid, Some(Signal::SIGKILL))?;
                        sent_sigkill = true;
                    }
                } else {
                    // DaemonIsRunningButHaveNoPid -- can reconstruct from report
                }
                // XX todo: write a "daemon stopped" message to log? From
                // here?  Or ignore signals in logging child and log it on
                // pipe close?
            } else {
                // Graceful stop or restart.
                if wait {
                    let mut i = 0;
                    loop {
                        sleep(Duration::from_millis(500));
                        // Do not just check if pid is none (daemon
                        // should be deleting pid as it goes down):
                        // restart action just sets another pid. Wait,
                        // actually does not change the pid if
                        // implemented by daemon re-exec'ing itself!
                        // But it will change a DaemonWant::Restart
                        // into a DaemonWant::Up. Also if any other
                        // actor changes the want we should stop,
                        // too. Thus, stop on *any* change of daemon
                        // state.
                        if daemon_state.access.load() != old_state {
                            break;
                        }
                        // Don't fully trust pid state changes
                        // (e.g. daemon crashing instead of shutting
                        // down cleanly), thus:
                        if i % 20 == 0 {
                            if !self.is_running()? {
                                // Should actually never happen for
                                // DaemonWant::Restart, right? Would
                                // indicate a crash, hence:
                                crashed = true;
                                break;
                            }
                        }
                        i += 1;
                    }
                }
            }
        }
        Ok(StopReport {
            was_pid,
            was_running,
            sent_sigint,
            sent_sigkill,
            crashed,
        })
    }

    /// Note: must be run while there are no running threads,
    /// otherwise panics! Returns the result of the `run` procedure in
    /// the child, but nothing in the parent.
    fn start(self) -> Result<ExecutionResult, DaemonError> {
        let daemon_state = self.daemon_state()?;
        let (current_want, current_pid) = daemon_state.read();

        // Try to get exclusive `is_running` lock. This can fail if
        // unlucky and a concurrent process tests with the shared
        // lock, thus retry.
        let lock_path = self.lock_path();
        let mut is_running_lock = {
            // Retry less often if there is indication that the daemon
            // is running as then failures are expected.
            let attempts =
                NonZeroU32::try_from(if current_want == DaemonWant::Up && current_pid.is_some() {
                    3
                } else {
                    30
                })
                .expect("nonzero");

            match retry_n(attempts, 10, || file_lock_nonblocking(&lock_path, true)) {
                Ok(lock) => lock,
                Err(e) => match e {
                    FileLockError::AlreadyLocked => {
                        match current_want {
                            DaemonWant::Down => {
                                // Signal that we want it to again be
                                // up; still have it effect the
                                // restart that would have happened
                                // anyway given more time.
                                retry(|| daemon_state.store_want(DaemonWant::Restart));
                            }
                            DaemonWant::Up => (),
                            DaemonWant::Restart => (),
                        }
                        // XX have a report as with stop?
                        return Ok(ExecutionResult::initiator());
                    }
                    _ => {
                        return Err(DaemonError::LockError {
                            lock_path: lock_path.into(),
                            error: e.to_string(),
                        })
                    }
                },
            }
        };

        daemon_state.want_starting();

        if let Some(_pid) = easy_fork().map_err(|error| DaemonError::ErrnoError {
            context: "fork",
            error,
        })? {
            // The child is holding onto the locks; apparently flock
            // acts globally when on the same filehandle, so we have
            // to disable the locks here in the parent.
            is_running_lock.leak();

            Ok(ExecutionResult::initiator())
        } else {
            // Start a new session, so that signals can be sent to
            // the whole group and will kill child processes, too.
            let session_pid = setsid().map_err(|error| DaemonError::ErrnoError {
                context: "setsid",
                error,
            })?;

            // Now write the new pid / session group leader to the
            // state file
            daemon_state.store(DaemonWant::Up, Some(session_pid.into()));

            // Start logging process
            let (logging_r, logging_w) = pipe().map_err(|error| DaemonError::ErrnoError {
                context: "pipe for logging",
                error,
            })?;
            if let Some(_logging_pid) = easy_fork().map_err(|error| DaemonError::ErrnoError {
                context: "forking the logger",
                error,
            })? {
                // In the daemon process.

                // Close the reading end of the pipe that we don't
                // use, and redirect stdout and stderr into the
                // pipe.
                close(logging_r).map_err(|error| DaemonError::ErrnoError {
                    context: "daemon: closing logging_r",
                    error,
                })?;
                dup2(logging_w, 1).map_err(|error| DaemonError::ErrnoError {
                    context: "daemon: dup to stdout",
                    error,
                })?;
                dup2(logging_w, 2).map_err(|error| DaemonError::ErrnoError {
                    context: "daemon: dup to stderr",
                    error,
                })?;
                close(logging_w).map_err(|error| DaemonError::ErrnoError {
                    context: "daemon: closing logging_w",
                    error,
                })?;

                eprintln!("daemon started");

                (self.run)(DaemonStateReader(
                    InnerDaemonStateReader::DaemonStateAccessor(&daemon_state),
                ));

                Ok(ExecutionResult::daemon(DaemonResult { daemon_state }))
            } else {
                // In the logging process.

                // Never writing from this process, close so that
                // we will detect when the daemon ends.
                close(logging_w).map_err(|error| DaemonError::ErrnoError {
                    context: "logger: closing logging_w",
                    error,
                })?;

                // Also, close stdout + stderr as those might be the
                // dup2 to another logger process, from before a
                // re-exec.
                _ = close(1);
                _ = close(2);

                match self.handle_logging(logging_r) {
                    Ok(()) => (),
                    Err(e) => {
                        // Will fail because we closed stderr. XX have
                        // some fail over logging location?
                        _ = write!(
                            &mut stderr(),
                            "logger process: ending because of error: {e}"
                        );
                    }
                }
                std::process::exit(0);
            }
        }
    }

    pub fn print_status(&self) -> anyhow::Result<()> {
        let daemon_state = self.daemon_state()?;
        let is_running = self.is_running()?;
        let (want, pid) = daemon_state.read();
        let is = if is_running { "running" } else { "stopped" };
        let mut out = stdout().lock();
        let pid_string;
        let pid_str = match pid {
            Some(pid) => {
                pid_string = format!("pid: {pid}, ");
                &pid_string
            }
            None => "",
        };
        writeln!(&mut out, "{is} ({pid_str}want: {want:?})")?;
        Ok(())
    }

    /// Note: must be run while there are no running threads--panics
    /// otherwise!
    pub fn execute(self, mode: DaemonMode) -> Result<ExecutionResult, DaemonError> {
        match mode {
            DaemonMode::Run => {
                (self.run)(DaemonStateReader(InnerDaemonStateReader::None));
                Ok(ExecutionResult::run())
            }
            DaemonMode::Start => Ok(self.start()?),
            DaemonMode::Stop(opts) => {
                let _report = self.stop_or_restartstop(DaemonWant::Down, opts)?;
                Ok(ExecutionResult::initiator())
            }
            DaemonMode::Restart(opts) => {
                let StopReport {
                    was_pid: _,
                    was_running,
                    sent_sigint,
                    sent_sigkill,
                    crashed,
                } = self.stop_or_restartstop(DaemonWant::Restart, opts)?;

                if !was_running || sent_sigint || sent_sigkill || crashed {
                    self.start()
                } else {
                    Ok(ExecutionResult::initiator())
                }
            }
            DaemonMode::Status => {
                self.print_status()?;
                Ok(ExecutionResult::initiator())
            }
            DaemonMode::STOP => {
                self.send_signal(Some(SIGSTOP))?;
                Ok(ExecutionResult::initiator())
            }
            DaemonMode::CONT => {
                self.send_signal(Some(SIGCONT))?;
                Ok(ExecutionResult::initiator())
            }
            DaemonMode::KILL => {
                self.send_signal(Some(SIGKILL))?;
                Ok(ExecutionResult::initiator())
            }
        }
    }

    fn handle_logging(&self, logging_r: i32) -> anyhow::Result<()> {
        // Put us in a new session again, to prevent the
        // logging from being killed when the daemon is,
        // so that we get all output and can log when the
        // daemon goes away.
        let _logging_session_pid = setsid()?;

        // XX add a fall back to copying to stderr if
        // logging fails (which may also happen due to
        // disk full!)

        self.create_dirs()?;

        // (Instead of BufReader and read_line, just read
        // chunks? No, since the sending side doesn't
        // actually buffer ~at all by default!)
        let mut messagesfh = BufReader::new(unsafe {
            // Safe because we're careful not to mess up
            // with the file descriptors (we're not giving
            // access to `messagesfh` from outside this
            // function, and we're not calling `close` on
            // this fd in this process)
            File::from_raw_fd(logging_r)
        });

        let mut input_line = String::new();
        let mut output_line = Vec::new();

        let mut logfh = open_append(self.current_log_path())?;
        let mut total_written: u64 = logfh.metadata()?.size();
        loop {
            input_line.clear();
            output_line.clear();
            let nread = messagesfh.read_line(&mut input_line)?;
            let daemon_ended = nread == 0;
            if self.opts.local_time {
                write!(&mut output_line, "{}", Local::now())?;
            } else {
                write!(&mut output_line, "{}", Utc::now())?;
            };
            writeln!(
                &mut output_line,
                "\t{}",
                if daemon_ended {
                    "daemon ended"
                } else {
                    input_line.trim_end()
                }
            )?;

            logfh.write_all(&output_line)?;
            total_written += output_line.len() as u64;

            if daemon_ended {
                break;
            }

            if total_written >= self.opts.max_log_file_size {
                logfh.flush()?; // well, not buffering anyway
                drop(logfh);
                self.rotate_logs()?;
                logfh = open_append(self.current_log_path())?;
                total_written = 0;
            }
        }
        logfh.flush()?; // well, not buffering anyway.
        Ok(())
    }
}

pub struct DaemonStateAccessor {
    path: Arc<Path>,
    access: IPCAtomicU64,
}

/// What state we want the daemon to be in
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonWant {
    Down,
    Up,
    /// Signals to the daemon that we want it to exit, but then be
    /// started again (it must set the state to Up on exit XX).
    Restart,
}

// Operations for daemon state, keep private!
impl DaemonWant {
    /// From DaemonState's AtomicU64. Ignores the lower half of the
    /// u64. Panics with `path` in the message for invalid values.
    fn from_u64(want: u64, path: &Arc<Path>) -> Self {
        let wantu32 = (want >> 32) as u32;
        if wantu32 == b'd' as u32 {
            DaemonWant::Down
        } else if wantu32 == b'u' as u32 {
            DaemonWant::Up
        } else if wantu32 == b'r' as u32 {
            DaemonWant::Restart
        } else {
            panic!(
                "got invalid upper value {wantu32} as DaemonWant value \
                 from DaemonState file {path:?}"
            )
        }
    }

    /// Ready to be used in DaemonState AtomicU64
    fn to_u64(self) -> u64 {
        let want = match self {
            DaemonWant::Down => b'd',
            DaemonWant::Up => b'u',
            DaemonWant::Restart => b'r',
        } as u32;
        (want as u64) << 32
    }
}

impl DaemonStateAccessor {
    pub fn open(path: Arc<Path>) -> Result<Self, IPCAtomicError> {
        let access = IPCAtomicU64::open(&path, (b'd' as u64) << 32)?;
        Ok(Self { path, access })
    }

    /// The second result is the pid if set. A pid present does not
    /// imply that the daemon is up--have to also check flock.
    pub fn read(&self) -> (DaemonWant, Option<i32>) {
        let v: u64 = self.access.load();
        let lower: u32 = v as u32;
        let pid = lower as i32;
        let pid = if pid == 0 { None } else { Some(pid) };

        let want = DaemonWant::from_u64(v, &self.path);
        (want, pid)
    }

    pub fn want(&self) -> DaemonWant {
        self.read().0
    }

    fn store(&self, want: DaemonWant, pid: Option<i32>) {
        let pid: u32 = pid.unwrap_or(0) as u32;
        let want = match want {
            DaemonWant::Down => b'd',
            DaemonWant::Up => b'u',
            DaemonWant::Restart => b'r',
        } as u32;
        let val = ((want as u64) << 32) + (pid as u64);
        self.access.store(val);
    }

    /// Change want while keeping pid field value. Returns the (old,
    /// new) value on success, or the newly attempted store in the
    /// error case, which means some change happened in the mean time,
    /// you could retry but may want to retry on a higher level
    /// instead.
    fn store_want(&self, want: DaemonWant) -> Result<(u64, u64), u64> {
        let wantu64 = want.to_u64();
        let atomic = self.access.atomic();
        let ordering = Ordering::SeqCst;

        let old = atomic.load(ordering);
        let new = (old & (u32::MAX as u64)) | wantu64;
        let got = atomic.compare_exchange(old, new, ordering, ordering)?;
        assert_eq!(got, old); // just testing my understanding--always guaranteed, right?
        Ok((old, new))
    }

    pub fn want_starting(&self) {
        self.store(DaemonWant::Up, None);
    }
}

enum InnerDaemonStateReader<'t> {
    DaemonStateAccessor(&'t DaemonStateAccessor),
    None,
}

pub struct DaemonStateReader<'t>(InnerDaemonStateReader<'t>);

impl<'t> DaemonStateReader<'t> {
    pub fn want(&self) -> DaemonWant {
        match self.0 {
            InnerDaemonStateReader::DaemonStateAccessor(daemon_state_accessor) => {
                daemon_state_accessor.want()
            }
            InnerDaemonStateReader::None => DaemonWant::Up,
        }
    }

    /// Whether the daemon should exit due to wanted Stop or Restart.
    pub fn want_exit(&self) -> bool {
        match self.want() {
            DaemonWant::Down => true,
            DaemonWant::Up => false,
            DaemonWant::Restart => true,
        }
    }
}
