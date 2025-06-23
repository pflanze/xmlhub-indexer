//! Infrastructure to run / start / stop a service as a daemon process
//! (or process group).

//! See [daemon](../docs/daemon.md) for some info.

use std::{
    fs::{create_dir, remove_file, rename, File},
    io::{stderr, stdout, BufRead, BufReader, ErrorKind, Read, Seek, Write},
    num::ParseIntError,
    os::{fd::FromRawFd, unix::prelude::MetadataExt},
    path::{Path, PathBuf},
    str::FromStr,
    thread::sleep,
    time::Duration,
};

use anyhow::{anyhow, bail, Context};
use chrono::{Local, Utc};
use nix::{
    sys::signal::{kill, Signal},
    unistd::{close, dup2, pipe, setsid, Pid},
};

use crate::{
    file_lock::{file_lock_nonblocking, FileLockError},
    file_util::{open_append, open_rw},
    path_util::AppendToPath,
    unix::{easy_flock_blocking, easy_fork},
};

#[derive(Debug, Clone, Copy)]
pub enum DaemonMode {
    /// Do not put into background, just run forever in the foreground.
    Run,
    /// Start daemon into background.
    Start,
    /// Same as Start but silently do nothing if daemon is already
    /// running.
    StartIfNotRunning,
    /// Stop existing daemon running in background (does not stop
    /// daemons in `Run` mode, only those in `Start` mode).
    Stop,
    /// `Stop` (ignoring errors) then `Start`.
    Restart,
    /// Report if there is a daemon in `Start` or `Run` mode.
    Status,
}

#[derive(thiserror::Error, Debug)]
#[error(
    "please give one of the strings `run`, `start`, `start-if-not-running`, \
     `stop`, `restart` or `status`"
)]
pub struct DaemonModeError;

impl FromStr for DaemonMode {
    type Err = DaemonModeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        {
            // reminder to adapt the code below when the enum changes
            match DaemonMode::Run {
                Run => (),
                Start => (),
                Stop => (),
                Restart => (),
                Status => (),
                StartIfNotRunning => (),
            }
        }
        use DaemonMode::*;
        match s {
            "run" => Ok(Run),
            "start" => Ok(Start),
            "start-if-not-running" => Ok(StartIfNotRunning),
            "stop" => Ok(Stop),
            "restart" => Ok(Restart),
            "status" => Ok(Status),
            _ => Err(DaemonModeError),
        }
    }
}

pub struct Daemon<P: AsRef<Path>, F: FnOnce() -> anyhow::Result<()>> {
    /// Where the lock/pid files and logs dir should be written to.
    pub base_dir: P,
    /// The code to run; the daemon ends/stops when this function
    /// returns.
    pub run: F,
    /// If true, logs in the local time zone,
    /// otherwise in UTC.
    pub use_local_time: bool,
    /// The maximum size of the current log file in bytes before it is
    /// being renamed and a new one opened.
    pub max_log_file_size: u64,
    /// The maximum number of numbered log files (i.e. excluding
    /// `current.log`) before the oldest are deleted.
    pub max_log_files: usize,
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
    path: PathBuf,
    error: InOutError,
}

#[derive(thiserror::Error, Debug)]
pub enum DaemonError {
    #[error("service {0:?} is already running")]
    AlreadyRunning(PathBuf),
    #[error("can't lock file {is_running_path:?}: {error}")]
    LockError {
        is_running_path: PathBuf,
        error: String,
    },
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
    Anyhow(#[from] anyhow::Error),
}

impl<P: AsRef<Path>, F: FnOnce() -> anyhow::Result<()>> Daemon<P, F> {
    pub fn create_dirs(&self) -> Result<(), PathIOError> {
        // XX add to file_util, including PathIOError perhaps?
        let create = |path| match create_dir(&path) {
            Ok(()) => Ok(()),
            Err(error) => match error.kind() {
                ErrorKind::AlreadyExists => Ok(()),
                _ => Err(PathIOError {
                    what: "create dir",
                    path,
                    error: error.into(),
                }),
            },
        };
        create(self.base_dir())?;
        create(self.log_dir())?;
        Ok(())
    }

    pub fn base_dir(&self) -> PathBuf {
        self.base_dir.as_ref().to_owned()
    }

    pub fn is_running_path(&self) -> PathBuf {
        self.base_dir().append("daemon_is_running.lock")
    }

    // Use a separate file since we need to lock this independently of
    // is_running.
    pub fn pid_path(&self) -> PathBuf {
        self.base_dir().append("pid")
    }

    pub fn log_dir(&self) -> PathBuf {
        let mut path: PathBuf = self.base_dir();
        path.push("logs");
        path
    }

    pub fn current_log_path(&self) -> PathBuf {
        self.log_dir().append("current.log")
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
        if num_numbered_logfiles > self.max_log_files {
            let delete_n = num_numbered_logfiles - self.max_log_files;
            for (_, path) in &numbered_logfiles[0..delete_n] {
                remove_file(path).with_context(|| anyhow!("deleting log file {path:?}"))?;
                // eprintln!("deleted log file {path:?}"); --ah, can't, no logging output
            }
        };
        Ok(())
    }

    /// The pid file could be empty (it is being emptied while `start`
    /// is holding a lock), but not when seeing `is_running` locked
    /// (since that means that a process that wrote the pid is alive,
    /// hence the pid must be there, and we don't get access to the
    /// pid file lock before it's been written). If it *is* empty, an
    /// error is returned.
    pub fn read_pid(&self) -> Result<i32, PathIOError> {
        let path = self.pid_path();
        let mut file = File::open(&path).map_err(|e| PathIOError {
            what: "open file",
            path: path.clone(),
            error: e.into(),
        })?;
        let mut lock = easy_flock_blocking(&mut file, false).map_err(|e| PathIOError {
            what: "lock file",
            path: path.clone(),
            error: e.into(),
        })?;
        let mut buf = String::new();
        lock.read_to_string(&mut buf).map_err(|e| PathIOError {
            what: "read file",
            path: path.clone(),
            error: e.into(),
        })?;
        drop(lock);
        buf.trim_end()
            .parse()
            .map_err(|e: ParseIntError| PathIOError {
                what: "parse file",
                path: path.clone(),
                error: e.into(),
            })
        // Should we return the lock, too? Should kill be done
        // while holding the lock?
    }

    pub fn is_running(&self) -> Result<bool, anyhow::Error> {
        let lock_path = self.is_running_path();
        // The daemon takes an exclusive lock; it's enough and
        // necessary to take a non-exclusive one here, so that
        // multiple testers don't find a lock by accident. XX test
        match file_lock_nonblocking(&lock_path, false) {
            Ok(lock) => {
                // We only get the (non-exclusive) lock as side effect
                // of our approach of testing for it being
                // locked. Drop it right away.
                drop(lock);
                Ok(false)
            }
            Err(e) => match e {
                FileLockError::AlreadyLocked => Ok(true),
                _ => bail!("lock error on {lock_path:?}: {e}"),
            },
        }
    }

    // Giving up and just using anyhow here
    pub fn stop(&self) -> Result<(), anyhow::Error> {
        if self.is_running()? {
            let pid: i32 = self.read_pid()?;
            if pid <= 0 {
                panic!("invalid pid {pid} is not > 0 in {:?}", self.pid_path());
            }
            let process_group_id: i32 = pid
                .checked_neg()
                .ok_or_else(|| anyhow!("pid {pid} can't be negated"))?;
            let kill_group_with = |signal| {
                match kill(Pid::from_raw(process_group_id), signal) {
                    Ok(()) => true,
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
                            false
                        }
                        _ => unreachable!(),
                    },
                }
            };
            if kill_group_with(Some(Signal::SIGINT)) {
                for _ in 0..40 {
                    sleep(Duration::from_millis(200));
                    if !kill_group_with(None) {
                        break;
                    }
                }
                kill_group_with(Some(Signal::SIGKILL));
            }
            // XX todo: write a "daemon stopped" message? From here?
            // Or ignore signals in logging child and log it on pipe
            // close?
        }
        Ok(())
    }

    /// Note: must be run while there are no running threads, panics
    /// otherwise!
    pub fn start(self) -> Result<(), DaemonError> {
        // 1. get exclusive lock on pid file; 2. get exclusive
        // `is_running` lock; 3. empty the pid file ASAP to invalidate
        // the stale pid (sigh, there's still a race here). 4. fork;
        // 5. in the child, write pid to it (that guarantees that when
        // the child gets to run `run`, the pid is written, OK?), and
        // let go of the pid lock. Readers are expected to check
        // is_running, then lock pid (non-exclusively) and read it.

        // 1. get exclusive lock on pid file, without modifying it yet
        let pid_path = self.pid_path();
        let mut pid_file = open_rw(&pid_path)?;
        let mut pid_lock =
            easy_flock_blocking(&mut pid_file, true).map_err(|error| DaemonError::ErrnoError {
                context: "flock",
                error,
            })?;

        // 2. get exclusive `is_running` lock
        let is_running_path = self.is_running_path();
        let mut is_running_lock = match file_lock_nonblocking(&is_running_path, true) {
            Ok(lock) => lock,
            Err(e) => match e {
                FileLockError::AlreadyLocked => {
                    return Err(DaemonError::AlreadyRunning(self.base_dir()))
                }
                _ => {
                    return Err(DaemonError::LockError {
                        is_running_path,
                        error: e.to_string(),
                    })
                }
            },
        };
        // 3. ASAP truncate the pid file (there is still a small race
        // window here!)
        pid_lock.set_len(0).map_err(|error| DaemonError::IoError {
            context: "pid_lock.set_len",
            error,
        })?;
        // Is this necessary?
        pid_lock
            .seek(std::io::SeekFrom::Start(0))
            .map_err(|error| DaemonError::IoError {
                context: "pid_lock.seek",
                error,
            })?;

        // 4. fork
        if let Some(_pid) = easy_fork().map_err(|error| DaemonError::ErrnoError {
            context: "fork",
            error,
        })? {
            // The child is holding onto the locks; apparently flock
            // acts globally when on the same filehandle, so we have
            // to disable the locks here.
            is_running_lock.leak();
            pid_lock.leak();

            Ok(())
        } else {
            match (|| -> anyhow::Result<()> {
                // Start a new session, so that signals can be sent to
                // the whole group and will kill child processes, too.
                let session_pid = setsid().map_err(|error| DaemonError::ErrnoError {
                    context: "setsid",
                    error,
                })?;

                // 5. write the new pid / session group leader to the pid file
                pid_lock.write_fmt(format_args!("{session_pid}\n"))?;
                pid_lock.flush()?;
                drop(pid_lock);

                // Start logging process
                let (logging_r, logging_w) = pipe()?;
                if let Some(_logging_pid) = easy_fork()? {
                    // In the daemon process.

                    // Close the reading end of the pipe that we don't
                    // use, and redirect stdout and stderr into the
                    // pipe.
                    close(logging_r)?;
                    dup2(logging_w, 1)?;
                    dup2(logging_w, 2)?;

                    eprintln!("daemon started");

                    (self.run)()?;
                } else {
                    // In the logging process.

                    // Put us in a new session again, to prevent the
                    // logging from being killed when the daemon is,
                    // so that we get all output and can log when the
                    // daemon goes away.
                    let _logging_session_pid = setsid()?;

                    // Never writing from this process, close so that
                    // we will detect when the daemon ends.
                    close(logging_w)?;

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
                        if self.use_local_time {
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

                        if total_written >= self.max_log_file_size {
                            logfh.flush()?; // well, not buffering anyway
                            drop(logfh);
                            self.rotate_logs()?;
                            logfh = open_append(self.current_log_path())?;
                            total_written = 0;
                        }
                    }
                    logfh.flush()?; // well, not buffering anyway.
                }
                Ok(())
            })() {
                Ok(()) => {
                    std::process::exit(0);
                }
                Err(e) => {
                    let _ = writeln!(&mut stderr(), "daemon terminated by error: {e:#}");
                    std::process::exit(1);
                }
            }
        }
    }

    pub fn print_status(&self) -> anyhow::Result<()> {
        let mut out = stdout().lock();
        let status = if self.is_running()? {
            "running"
        } else {
            "stopped"
        };
        writeln!(&mut out, "{status}")?;
        Ok(())
    }

    /// Note: must be run while there are no running threads, panics
    /// otherwise!
    pub fn execute(self, mode: DaemonMode) -> anyhow::Result<()> {
        match mode {
            DaemonMode::Run => {
                (self.run)()?;
            }
            DaemonMode::Start => {
                self.start()?;
            }
            DaemonMode::StartIfNotRunning => match self.start() {
                Ok(()) => (),
                Err(DaemonError::AlreadyRunning(_)) => (),
                Err(e) => Err(e)?,
            },
            DaemonMode::Stop => {
                self.stop()?;
            }
            DaemonMode::Restart => {
                self.stop()?;
                self.start()?;
            }
            DaemonMode::Status => {
                self.print_status()?;
            }
        }
        Ok(())
    }
}
