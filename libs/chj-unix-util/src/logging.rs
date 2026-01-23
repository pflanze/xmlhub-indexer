use std::{
    ffi::CString,
    fs::{remove_file, rename, File},
    io::{stderr, BufRead, BufReader, Write},
    os::{fd::FromRawFd, unix::fs::MetadataExt},
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
    time::SystemTime,
};

use anyhow::{anyhow, Context};
use cj_path_util::path_util::AppendToPath;
use nix::{
    libc::{prctl, PR_SET_NAME},
    unistd::{close, dup2, pipe, setsid, Pid},
};

use crate::{
    daemon::DaemonError, file_util::open_append, timestamp_formatter::TimestampFormatter,
    unix::easy_fork,
};

// Expecting a tab between timestamp and the rest of the line! Also,
// expecting no slashes.
pub fn starts_with_timestamp(line: &str) -> bool {
    let mut digits = 0;
    let mut minus = 0;
    let mut plus = 0;
    let mut t = 0;
    let mut slash = 0;
    let mut colon = 0;
    let mut space = 0;
    let mut dot = 0;
    let mut other = 0;
    for (i, c) in line.chars().enumerate() {
        if i > 40 {
            return false;
        }
        if c == '\t' {
            return other <= 5
                && digits >= 16
                && slash == 0
                && colon >= 2
                && dot <= 1
                && space <= 2
                && minus <= 3
                && plus <= 1
                && t <= 1;
        }
        if c.is_ascii_digit() {
            digits += 1;
        } else if c == '-' {
            minus += 1;
        } else if c == ':' {
            colon += 1;
        } else if c == ' ' {
            space += 1;
        } else if c == '+' {
            plus += 1;
        } else if c == 'T' {
            t += 1;
        } else if c == '/' {
            slash += 1;
        } else if c == '.' {
            dot += 1;
        } else {
            other += 1;
        }
    }
    // found no tab
    false
}

#[test]
fn t_starts_with_timestamp() {
    let cases = [
        ("2026-01-11 15:25:12.000445551 +01:00	src/run/working_directory_pool.rs:552:17	process_working_directory D40 (None for test-running versioned dataset search at_2026-01-11T15:25:11.936345998+01:00) succeeded.", true),
        ("2026-01-11T15:25:12.000319897+01:00	src/run/working_directory_pool.rs:552:17	process_working_directory D40 (None for test-running versioned dataset search at_2026-01-11T15:25:11.936345998+01:00) succeeded.", true),
        ("src/run/working_directory_pool.rs:552:17	process_working_directory D40 (None for test-running versioned dataset search at_2026-01-11T15:25:11.936345998+01:00) succeeded.", false),
    ];
    for (s, expected) in &cases {
        assert!(
            starts_with_timestamp(s) == *expected,
            "{s:?} to yield {expected:?}"
        );
    }
}

#[derive(Debug, Clone)]
pub enum TimestampMode {
    /// Always add a timestamp
    Always,
    /// Add a timestamp if none is found, checking each line
    Automatic {
        /// Whether a "," is appended to timestamps that the logger
        /// daemon adds (could be useful if the app itself (usually)
        /// also adds timestamps, to be able to distinguish, since
        /// there is a time delay for the times from the logger
        /// daemon).
        mark_added_timestamps: bool,
    },
    /// Never add timestamps, assumes the application reliably adds
    /// them
    Never,
}

/// These settings will better be set statically by the app, rather
/// than exposed to the user, thus not deriving clap::Args.
#[derive(Debug, Clone)]
pub struct TimestampOpts {
    /// Whether rfc3339 format is to be used (default: whatever chrono
    /// uses for `Display`).
    pub use_rfc3339: bool,

    pub mode: TimestampMode,
}

#[derive(Debug, Clone, clap::Args)]
pub struct LoggingOpts {
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

pub struct Logger {
    pub logging_opts: LoggingOpts,
    pub timestamp_opts: TimestampOpts,
    pub dir_path: Arc<Path>,
}

impl Logger {
    pub fn current_log_path(&self) -> PathBuf {
        self.dir_path.append("current.log")
    }

    /// Rename the "current.log" file (if present) to "000001.log" or
    /// similar, allocating a new number, and delete old log files if
    /// there are more than configured.
    pub fn rotate_logs(&self) -> anyhow::Result<()> {
        let mut numbered_logfiles = Vec::new();
        for entry in std::fs::read_dir(&self.dir_path)? {
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
        let new_log_path = (&self.dir_path).append(&format!("{new_number:06}.log"));
        let current_log_path = self.current_log_path();
        match rename(&current_log_path, &new_log_path) {
            Ok(_) => numbered_logfiles.push((new_number, new_log_path)),
            Err(_) => (), // guess there's no file? XX look into what error it is
        };
        let num_numbered_logfiles = numbered_logfiles.len();
        if let Some(max_log_files) = self.logging_opts.max_log_files {
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

    fn run_logger_proxy(&self, logging_r: i32, session_pid: Pid) -> anyhow::Result<()> {
        // Put us in a new session again, to prevent the
        // logging from being killed when the daemon is,
        // so that we get all output and can log when the
        // daemon goes away.
        let _logging_session_pid = setsid()?;

        // XX add a fall back to copying to stderr if
        // logging fails (which may also happen due to
        // disk full!)

        let timestamp_formatter = TimestampFormatter {
            use_rfc3339: self.timestamp_opts.use_rfc3339,
            local_time: self.logging_opts.local_time,
        };

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
            let (starts_with_timestamp, mark_added_timestamps) = match &self.timestamp_opts.mode {
                TimestampMode::Always => (false, false),
                TimestampMode::Automatic {
                    mark_added_timestamps,
                } => (starts_with_timestamp(&input_line), *mark_added_timestamps),
                TimestampMode::Never => (true, false),
            };
            if !starts_with_timestamp {
                let s = timestamp_formatter.format_systemtime(SystemTime::now());
                _ = output_line.write_all(s.as_bytes());
                if mark_added_timestamps {
                    // The comma is used (and not e.g. the dot) since
                    // it isn't part of double click auto-selection in
                    // my terminal, and because it may be treated as a
                    // column separator (together with '\t'), and
                    // shifting column in a spreadsheet is perhaps
                    // better for seeing the difference while allowing
                    // the timestamps to be identical to the normal
                    // ones for parsing.
                    output_line.push(b',');
                }
                output_line.push(b'\t');
            }
            {
                let tmp;
                let rest = if daemon_ended {
                    tmp = format!("daemon {session_pid} ended");
                    &tmp
                } else {
                    input_line.trim_end()
                };
                _ = output_line.write_all(rest.as_bytes());
                output_line.push(b'\n');
            }

            logfh.write_all(&output_line)?;
            total_written += output_line.len() as u64;

            if daemon_ended {
                break;
            }

            if total_written >= self.logging_opts.max_log_file_size {
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

    /// Fork off a logger process (it immediately starts a new unix
    /// session to avoid being killed when the parent process group is
    /// killed) and redirect stdout and stderr to it. `session_pid` is
    /// denoting the daemon instance (the parent process or rather
    /// process group), it is written to the log when the parent ends
    /// (or closes stderr and stdout). Make sure stdout and stderr are
    /// flushed before calling this method.
    pub fn redirect_to_logger(self, session_pid: Pid) -> Result<(), DaemonError> {
        // Start logging process
        let (logging_r, logging_w) = pipe().map_err(|error| DaemonError::ErrnoError {
            context: "pipe for logging",
            error,
        })?;

        if let Some(_logging_pid) = easy_fork().map_err(|error| DaemonError::ErrnoError {
            context: "forking the logger",
            error,
        })? {
            // In the parent process.

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

            Ok(())
        } else {
            // In the logging process.

            // Make it visible that this is the logger. ('ps'
            // doesn't show it, but `head -1 /proc/$pid/status `
            // does.)
            {
                // Leak it to ensure it stays around? XX Is this necessary?
                let name = Box::leak(Box::new(CString::new("logger").expect("compatible")));
                unsafe {
                    // Safe as long as `name` stays around?
                    prctl(PR_SET_NAME, (&**name).as_ptr(), 0, 0, 0);
                }
            }

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

            match self.run_logger_proxy(logging_r, session_pid) {
                Ok(()) => (),
                Err(e) => {
                    // Will fail because we closed stderr. XX have
                    // some fail over logging location?
                    _ = writeln!(
                        &mut stderr(),
                        "logger process: ending because of error: {e:#}"
                    );
                }
            }
            std::process::exit(0);
        }
    }
}
