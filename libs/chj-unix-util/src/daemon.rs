//! Infrastructure to run / start / stop a service as a daemon process
//! (or process group).

//! See [daemon](../docs/daemon.md) for more info.

pub mod warrants_restart;

use std::{
    borrow::Cow,
    fmt::Debug,
    io::{stderr, stdout, Write},
    num::{NonZeroU32, ParseIntError},
    ops::Deref,
    os::unix::ffi::OsStrExt,
    path::Path,
    str::FromStr,
    sync::{atomic::Ordering, Arc},
    thread::sleep,
    time::Duration,
};

use anyhow::{anyhow, bail, Context, Result};
use cj_path_util::path_util::AppendToPath;
use nix::{
    libc::getsid,
    sys::signal::Signal::{self, SIGCONT, SIGKILL, SIGSTOP},
    unistd::{execvp, setsid, Pid},
};

use crate::{
    backoff::LoopWithBackoff,
    daemon::warrants_restart::WarrantsRestart,
    eval_with_default::EvalWithDefault,
    file_lock::{file_lock_nonblocking, FileLockError},
    file_util::{create_dir_if_not_exists, PathIOError},
    forking_loop::forking_loop,
    logging::{Logger, LoggingOpts, TimestampOpts},
    polling_signals::{IPCAtomicError, IPCAtomicU64},
    re_exec::re_exec,
    retry::{retry, retry_n},
    signal::send_signal_to_all_processes_of_session,
    unix::easy_fork,
    util::cstring,
};

/// You may want to use this as a normal argument, i.e. via FromStr,
/// instead, as then single strings can be passed through. There is
/// more flexibility with the options here, though. XX currently this
/// is also conflating STOP and Stop etc., and does not have `up`
/// etc. aliases, thus pretty unusable.
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

    /// Report if there is a daemon in `start` or `run` mode, together
    /// with pid if running and what the desired status is.
    Status,

    /// Report if there is a daemon in `start` or `run` mode (without
    /// additional information).
    ShortStatus,

    /// Send a STOP signal to (i.e. suspend) the daemon.
    STOP,

    /// Send a CONT signal to (i.e. continue) the daemon.
    CONT,

    /// Send a KILL signal to (i.e. terminate right away) the daemon.
    KILL,

    /// Open the current log file in the pager ($PAGER or 'less')
    Log,

    /// Run `tail -f` on the current log file
    Logf,
}

const FROM_STR_CASES: &[(&str, DaemonMode, &str)] = {
    const fn opts(hard: bool, soft: bool) -> StopOpts {
        StopOpts {
            hard,
            soft,
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
            ShortStatus => (),
            STOP => (),
            CONT => (),
            KILL => (),
            Log => (),
            Logf => (),
        }
    }

    &[
        (
            "run",
            DaemonMode::Run,
            "Do not put into background, just run forever in the foreground.",
        ),
        ("start", DaemonMode::Start, "Start daemon into background."),
        ("up", DaemonMode::Start, "Alias for `start`."),
        (
            "stop",
            DaemonMode::Stop(opts(false, false)),
            "Stop a daemon running in background (does not stop daemons running\n\
             via `run`). Alias for hard-stop or soft-stop, depending on the application.",
        ),
        (
            "hard-stop",
            DaemonMode::Stop(opts(true, false)),
            "Stop daemon by sending it and its children signals (SIGINT then SIGKILL).\n\
             Returns only after the daemon has ended.",
        ),
        (
            "soft-stop",
            DaemonMode::Stop(opts(false, true)),
            "Stop daemon gracefully by sending the daemon a plea to exit. Returns\n\
             immediately, the daemon will stop at its own leisure.",
        ),
        (
            "down",
            DaemonMode::Stop(opts(false, false)),
            "Alias for `stop`.",
        ),
        (
            "hard-down",
            DaemonMode::Stop(opts(true, false)),
            "Alias for `hard-stop`.",
        ),
        (
            "soft-down",
            DaemonMode::Stop(opts(false, true)),
            "Alias for `soft-stop`.",
        ),
        (
            "restart",
            DaemonMode::Restart(opts(false, false)),
            "Alias for `hard-restart` or `soft-restart`, depending on the application.",
        ),
        (
            "hard-restart",
            DaemonMode::Restart(opts(true, false)),
            "`hard-stop` then `start` the daemon; picks up new command line flags and\n\
             environment changes.",
        ),
        (
            "soft-restart",
            DaemonMode::Restart(opts(false, true)),
            "Sends the daemon a plea to re-execute itself, with its original command line\n\
             flags and environment.",
        ),
        (
            "status",
            DaemonMode::Status,
            "Show if a (start/stop based) daemon is running, with pid (if running) and the\n\
             desired status.",
        ),
        (
            "short-status",
            DaemonMode::ShortStatus,
            "Show if a (start/stop based) daemon is running in one word.",
        ),
        (
            "STOP",
            DaemonMode::STOP,
            "Send a STOP signal to the daemon and its children.",
        ),
        (
            "CONT",
            DaemonMode::CONT,
            "Send a CONT signal to the daemon and its children.",
        ),
        (
            "KILL",
            DaemonMode::KILL,
            "Send a KILL signal to the daemon and its children.",
        ),
        (
            "log",
            DaemonMode::Log,
            "Open the current log file in the pager ($PAGER or 'less')",
        ),
        (
            "logf",
            DaemonMode::Logf,
            "Run `tail -f` on the current log file",
        ),
    ]
};

fn errmsg() -> String {
    // Cannot do join() since not using itertools in this crate.
    let mut s = String::from("please give one of the following arguments:\n\n");
    for (k, _m, doc) in FROM_STR_CASES {
        use std::fmt::Write;
        _ = writeln!(&mut s, "    `{k}`:");
        for line in doc.split('\n') {
            _ = writeln!(&mut s, "        {line}");
        }
        s.push('\n');
    }

    s.truncate(s.len() - 2);
    s
}

#[derive(thiserror::Error, Debug)]
#[error("{}", errmsg())]
pub struct DaemonModeError;

impl FromStr for DaemonMode {
    type Err = DaemonModeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        for (k, v, _doc) in FROM_STR_CASES {
            if s == *k {
                return Ok(v.clone());
            }
        }
        Err(DaemonModeError)
    }
}

#[derive(Debug, Clone, Copy, Default, clap::Args)]
#[clap(global_setting(clap::AppSettings::DeriveDisplayOrder))]
pub struct RestartOnFailures {
    // Adding `, help = None` does not help to avoid the empty paragraph
    #[clap(long)]
    pub restart_on_failures: bool,

    /// Whether to restart the daemon or not when it crashes (in
    /// start/up mode).  Restarting works by forking before running
    /// the work, then re-forking when the child ends in a non-normal
    /// way (exit with an error or by signal). Between restarts, the
    /// parent sleeps a bit, with exponential back-off within a time
    /// range configured by the application. The default restart
    /// behaviour is set by the application; the two options cancel
    /// each other out. Note: debugging crashes will be easiest using
    /// the `run` mode, which ignores the restart setting (never
    /// restarts).
    #[clap(long)]
    pub no_restart_on_failures: bool,
}

impl EvalWithDefault for RestartOnFailures {
    fn explicit_yes_and_no(&self) -> (bool, bool) {
        let Self {
            restart_on_failures,
            no_restart_on_failures,
        } = self;
        (*restart_on_failures, *no_restart_on_failures)
    }
}

/// These settings may be useful to expose to the user.
#[derive(Debug, Clone, Default, clap::Args)]
pub struct DaemonOpts {
    #[clap(flatten)]
    pub logging_opts: LoggingOpts,

    #[clap(flatten)]
    pub restart_on_failures: RestartOnFailures,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonPaths {
    /// Where the lock/pid files should be written to (is created if missing).
    pub state_dir: Arc<Path>,
    /// Where the log files should be written to (is created if missing).
    pub log_dir: Arc<Path>,
}

pub struct Daemon<
    Other: Deref<Target: WarrantsRestart> + Clone,
    F: FnOnce(DaemonCheckExit<Other>) -> Result<()>,
> {
    pub opts: DaemonOpts,
    /// The default value for opts.restart_on_failures.eval()
    pub restart_on_failures_default: bool,
    /// The settings for the restarting; if not provided, uses its
    /// Default values. The `daemon` field is overwritten with the
    /// string "daemon service process restart ".
    pub restart_opts: Option<LoopWithBackoff>,
    pub timestamp_opts: TimestampOpts,
    /// The code to run; the daemon ends/stops when this function
    /// returns. The function should periodically call `want()` on its
    /// argument and stop processing when it doesn't give
    /// `DaemonWant::Up`.
    pub paths: DaemonPaths,
    /// A value that implements `WarrantsRestart`, checking *other*
    /// conditions warranting restart than the daemon state indicating
    /// it. Used as part of the argument to `run`. See
    /// `chj_unix_util::daemon::warrants_restart` for reusable
    /// implementations.
    pub other_restart_checks: Other,
    /// The code to run in the daemon. Should return when calling
    /// `want_exit()` on the argument returns true.
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
    PathIOError(#[from] PathIOError),
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
                let e = re_exec();
                eprintln!("{e:#}");
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
    /// This stops the daemon via signals, first SIGINT, then
    /// SIGKILL. Restarting in this mode takes the new enviroment from
    /// the issuer since it works by forking a new daemon.  (--hard
    /// and --soft are opposites; the default depends on the
    /// application.)
    // `short` -h conflicts with --help
    #[clap(long)]
    pub hard: bool,

    /// This stops the daemon by communicating a wish for termination
    /// via shared memory. The daemon may delay the reaction for a
    /// long time. Restarting in this mode works by the daemon
    /// re-executing itself, meaning it will not pick up environment
    /// or command line argument changes. This action returns
    /// immediately as it only stores the wish. (--hard and --soft are
    /// opposites; the default depends on the application.)
    #[clap(short, long)]
    pub soft: bool,

    /// When doing graceful termination, by default the stop/restart
    /// actions do not wait for the daemon to carry it out. This
    /// changes the behaviour to wait in that case, too.
    #[clap(short, long)]
    pub wait: bool,

    /// The time in seconds after sending SIGINT before sending
    /// SIGKILL
    // Default: keep in sync with const fn opts
    #[clap(short, long, default_value = "30")]
    pub timeout_before_sigkill: u32,
}

impl StopOpts {
    pub fn hard(&self, default_is_hard: bool) -> bool {
        let Self {
            hard,
            soft,
            wait: _,
            timeout_before_sigkill: _,
        } = self;
        match (hard, soft) {
            (false, false) | (true, true) => default_is_hard,
            (true, false) => true,
            (false, true) => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StopReport {
    pub was_pid: Option<i32>,
    pub was_running: bool,
    pub sent_sigint: bool,
    pub sent_sigkill: bool,
    pub crashed: bool,
}

impl<
        Other: Deref<Target: WarrantsRestart> + Clone,
        F: FnOnce(DaemonCheckExit<Other>) -> Result<()>,
    > Daemon<Other, F>
{
    pub fn create_dirs(&self) -> Result<(), PathIOError> {
        create_dir_if_not_exists(&self.state_dir())?;
        create_dir_if_not_exists(&self.log_dir())?;
        Ok(())
    }

    pub fn state_dir(&self) -> Arc<Path> {
        self.paths.state_dir.clone()
    }

    pub fn log_dir(&self) -> Arc<Path> {
        self.paths.log_dir.clone()
    }

    pub fn to_logger(&self) -> Logger {
        Logger {
            logging_opts: self.opts.logging_opts.clone(),
            timestamp_opts: self.timestamp_opts.clone(),
            dir_path: self.log_dir(),
        }
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

    fn daemon_state(&self) -> anyhow::Result<DaemonStateAccessor> {
        let daemon_state_path = self.daemon_state_path();
        DaemonStateAccessor::open(daemon_state_path.clone())
            .with_context(|| anyhow!("opening {daemon_state_path:?}"))
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
                _ => bail!("lock error on {lock_path:?}: {e:#}"),
            },
        }
    }

    /// Send the signal once or twice (once via the process group,
    /// then individually if still around) to all processes belonging
    /// to the session that the daemon is running in.
    pub fn send_signal(&self, signal: Option<Signal>) -> anyhow::Result<bool> {
        let daemon_state = self.daemon_state()?;
        let (_old_want, was_pid) = daemon_state.read();
        if let Some(session_pid) = was_pid {
            retry(|| daemon_state.store_want(DaemonWant::Down));
            let session_pid = Pid::from_raw(session_pid);
            send_signal_to_all_processes_of_session(session_pid, signal)
        } else {
            Ok(false)
        }
    }

    // (Giving up and just using anyhow here)
    fn stop_or_restartstop(
        &self,
        want: DaemonWant,
        opts: StopOpts,
        default_is_hard: bool,
    ) -> Result<StopReport, anyhow::Error> {
        let StopOpts {
            hard: _,
            soft: _,
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
            if opts.hard(default_is_hard) {
                if let Some(session_pid) = was_pid {
                    let session_pid = Pid::from_raw(session_pid);
                    if send_signal_to_all_processes_of_session(session_pid, Some(Signal::SIGINT))? {
                        sent_sigint = true;
                        let sleep_duration_ms: u64 = 1000;
                        let num_sleeps =
                            u64::from(timeout_before_sigkill) * 1000 / sleep_duration_ms;
                        'outer: {
                            for _ in 0..num_sleeps {
                                sleep(Duration::from_millis(sleep_duration_ms));
                                if !send_signal_to_all_processes_of_session(session_pid, None)? {
                                    break 'outer;
                                }
                            }
                            send_signal_to_all_processes_of_session(
                                session_pid,
                                Some(Signal::SIGKILL),
                            )?;
                            sent_sigkill = true;
                        }
                        // Remove the pid
                        daemon_state.store(want, None);
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
        if self
            .opts
            .restart_on_failures
            .eval_with_default(self.restart_on_failures_default)
        {
            // Wrap `run`
            let Daemon {
                opts,
                restart_on_failures_default: _,
                restart_opts,
                timestamp_opts,
                paths,
                other_restart_checks,
                run,
            } = self;

            let run = |daemon_check_exit: DaemonCheckExit<Other>| -> Result<()> {
                let mut opts = restart_opts.unwrap_or_else(Default::default);
                opts.prefix = "daemon service process restart ".into();
                forking_loop(
                    opts,
                    || -> Result<()> { run(daemon_check_exit.clone()) },
                    || daemon_check_exit.want_exit(),
                );
                Ok(())
            };

            // The wrapper does not need yet another layer for
            // restarting (although, the `_start` method ignores that
            // anyway)
            let opts = DaemonOpts {
                restart_on_failures: RestartOnFailures {
                    restart_on_failures: false,
                    no_restart_on_failures: true,
                },
                ..opts
            };

            Daemon {
                opts,
                restart_on_failures_default: false,
                restart_opts: None,
                timestamp_opts,
                paths,
                other_restart_checks,
                run,
            }
            ._start()
        } else {
            self._start()
        }
    }

    fn _start(self) -> Result<ExecutionResult, DaemonError> {
        self.create_dirs()?;

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

            let logger = self.to_logger();
            logger.redirect_to_logger(session_pid)?;

            eprintln!("daemon {session_pid} started");

            (self.run)(DaemonCheckExit(Some((
                DaemonStateReader(&daemon_state),
                self.other_restart_checks,
            ))))?;

            Ok(ExecutionResult::daemon(DaemonResult { daemon_state }))
        }
    }

    pub fn status_string(&self, additional_info: bool) -> anyhow::Result<Cow<'static, str>> {
        let daemon_state = self.daemon_state()?;
        let is_running = self.is_running()?;
        let (want, pid) = daemon_state.read();
        let is = if is_running { "running" } else { "stopped" };
        if additional_info {
            let pid_string = match pid {
                Some(pid) => {
                    format!("pid: {pid}, ")
                }
                None => "".into(),
            };
            Ok(format!("{is} ({pid_string}want: {want:?})").into())
        } else {
            Ok(is.into())
        }
    }

    pub fn print_status(&self, additional_info: bool) -> anyhow::Result<()> {
        let s = self.status_string(additional_info)?;
        (|| -> Result<()> {
            let mut out = stdout().lock();
            out.write_all(s.as_bytes())?;
            out.write_all(b"\n")?;
            out.flush()?;
            Ok(())
        })()
        .context("printing to stdout")
    }

    /// Note: actions involving forking a new instance must be run
    /// while there are no running threads--they panic otherwise!
    pub fn execute(
        self,
        mode: DaemonMode,
        default_is_hard: bool,
    ) -> Result<ExecutionResult, DaemonError> {
        match mode {
            DaemonMode::Run => {
                (self.run)(DaemonCheckExit(None))?;
                Ok(ExecutionResult::run())
            }
            DaemonMode::Start => Ok(self.start()?),
            DaemonMode::Stop(opts) => {
                let _report = self.stop_or_restartstop(DaemonWant::Down, opts, default_is_hard)?;
                Ok(ExecutionResult::initiator())
            }
            DaemonMode::Restart(opts) => {
                let StopReport {
                    was_pid: _,
                    was_running,
                    sent_sigint,
                    sent_sigkill,
                    crashed,
                } = self.stop_or_restartstop(DaemonWant::Restart, opts, default_is_hard)?;

                if !was_running || sent_sigint || sent_sigkill || crashed {
                    self.start()
                } else {
                    Ok(ExecutionResult::initiator())
                }
            }
            DaemonMode::Status => {
                self.print_status(true)?;
                Ok(ExecutionResult::initiator())
            }
            DaemonMode::ShortStatus => {
                self.print_status(false)?;
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
            DaemonMode::Log => {
                // Once again.
                let cmd = match std::env::var_os("PAGER") {
                    Some(path) => cstring(path.as_bytes())?,
                    None => cstring("less")?,
                };
                execvp(
                    &cmd,
                    &[
                        &cmd,
                        &cstring(
                            self.to_logger()
                                .current_log_path()
                                .into_os_string()
                                .as_bytes(),
                        )?,
                    ],
                )
                .with_context(|| anyhow!("exec'ing {cmd:?} command"))?;
                unreachable!("execv never returns Ok")
            }
            DaemonMode::Logf => {
                let cmd = cstring("tail")?;
                execvp(
                    &cmd,
                    &[
                        &cmd,
                        &cstring("-f")?,
                        &cstring(
                            self.to_logger()
                                .current_log_path()
                                .into_os_string()
                                .as_bytes(),
                        )?,
                    ],
                )
                .context("exec'ing `tail` command")?;
                unreachable!("execv never returns Ok")
            }
        }
    }
}

#[derive(Debug)]
pub struct DaemonStateAccessor {
    path: Arc<Path>,
    access: IPCAtomicU64,
}

/// What state we want the daemon to be in
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonWant {
    Down,
    Up,
    /// Signals to the daemon that we want it to re-execute itself
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

    /// Whether the value warrants exiting from a daemon
    pub fn wants_exit(self) -> bool {
        match self {
            DaemonWant::Down => true,
            DaemonWant::Up => false,
            DaemonWant::Restart => true,
        }
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
        // just testing my understanding--always guaranteed, right?
        if !(got == old) {
            _ = writeln!(
                &mut stderr(),
                "got != old, {} vs. {} at {}:{}",
                got,
                old,
                file!(),
                line!()
            );
        }
        Ok((old, new))
    }

    pub fn want_starting(&self) {
        self.store(DaemonWant::Up, None);
    }
}

#[derive(Debug, Clone)]
pub struct DaemonStateReader<'t>(&'t DaemonStateAccessor);

impl<'t> DaemonStateReader<'t> {
    pub fn want(&self) -> DaemonWant {
        self.0.want()
    }

    /// Whether the daemon should exit due to wanted Stop or Restart.
    pub fn want_exit(&self) -> bool {
        self.want().wants_exit()
    }
}

#[derive(Debug, Clone)]
pub struct DaemonCheckExit<'t, Other: Deref<Target: WarrantsRestart> + Clone>(
    Option<(DaemonStateReader<'t>, Other)>,
);

impl<'t, Other: Deref<Target: WarrantsRestart> + Clone> DaemonCheckExit<'t, Other> {
    pub fn want_exit(&self) -> bool {
        if let Some((daemon_state_reader, other)) = &self.0 {
            daemon_state_reader.want_exit() || {
                if other.warrants_restart() {
                    // Already store the change in want, so that
                    // forking_loop or whichever upper levels don't
                    // have to re-evaluate secondary checks again (and
                    // trigger duplicate notifications). (Also, maybe
                    // this is better in case the app crashes while
                    // restarting?)
                    retry(|| daemon_state_reader.0.store_want(DaemonWant::Restart));
                    true
                } else {
                    false
                }
            }
        } else {
            false
        }
    }
}
