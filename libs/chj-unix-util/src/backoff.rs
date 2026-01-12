//! Run a job in an interval that is increased if there are errors,
//! and lowered again if there aren't.

//! Todo: measure time and subtract the job running time from the
//! sleep time? Or at least, when gettng an error, take the passed
//! time as the basis, not the `min_sleep_seconds` time (since that
//! might be 0, even).

use std::{
    fmt::Display,
    thread::sleep,
    time::{Duration, SystemTime},
};

/// Whether the loop should give additional messaging about its own
/// working (this excludes messages about handling errors, and is just
/// about reporting on normal working)
pub enum LoopVerbosity {
    Silent,
    LogEveryIteration,
    /// NOTE: the interval is at *least* as long; it is never shorter
    /// than the loop sleep time.
    LogActivityInterval {
        every_n_seconds: u64,
    },
}

/// Configuration data; implements `Default` so you can initialize an
/// instance setting only the fields that you want to change (most
/// likely only the `*_sleep_seconds` values).
pub struct LoopWithBackoff {
    /// Whether to enable additional diagnostic messages to stderr
    /// (default: `LoopVerbosity::LogSleepTimeEveryIteration`).
    pub verbosity: LoopVerbosity,
    /// Whether to silence diagnostic messages to stderr about errors
    /// (default: false).
    pub quiet: bool,
    /// The number to multiply the sleep time with in case of error (should be > 1)
    pub error_sleep_factor: f64,
    /// The number to multiply the sleep time with in case of success
    /// (should be between 0 and 1)
    pub success_sleep_factor: f64,
    /// The number of seconds to sleep at minimum (do not use 0 since
    /// then it will never back off!)
    pub min_sleep_seconds: f64,
    /// The number of seconds to sleep at maximum (should be >
    /// `min_sleep_seconds`).
    pub max_sleep_seconds: f64,
}

impl Default for LoopWithBackoff {
    fn default() -> Self {
        Self {
            verbosity: LoopVerbosity::LogEveryIteration,
            quiet: false,
            error_sleep_factor: 1.05,
            success_sleep_factor: 0.99,
            min_sleep_seconds: 1.,
            max_sleep_seconds: 1000.,
        }
    }
}

impl LoopWithBackoff {
    /// Loop running `job` then sleeping at least `min_seconds`, if
    /// `job` returns an `Err`, increases the sleep time. Runs `until`
    /// after every run, and returns if it returns true.
    pub fn run<E: Display>(
        &self,
        mut job: impl FnMut() -> Result<(), E>,
        until: impl Fn() -> bool,
    ) {
        let mut sleep_seconds = self.min_sleep_seconds;
        let mut iteration_count: u64 = 0;
        let mut last_lai_time: Option<SystemTime> = None;
        loop {
            let result = job();
            if let Err(e) = result {
                // XX e:# ? but we only have Display! Can't require
                // std::error::Error since anyhow::Error (in the
                // version that I'm using) is not implementing that.
                if !self.quiet {
                    eprintln!("control loop: got error: {e:#}");
                }
                sleep_seconds =
                    (sleep_seconds * self.error_sleep_factor).min(self.max_sleep_seconds);
            } else {
                sleep_seconds = (sleep_seconds * 0.99).max(self.min_sleep_seconds);
            }
            if until() {
                return;
            }
            let verbose_print = || {
                eprintln!(
                    "loop iteration {iteration_count}, \
                     sleeping {sleep_seconds} seconds"
                )
            };
            match self.verbosity {
                LoopVerbosity::Silent => (),
                LoopVerbosity::LogEveryIteration => {
                    verbose_print();
                }
                LoopVerbosity::LogActivityInterval { every_n_seconds } => {
                    let now = SystemTime::now();
                    if let Some(last) = last_lai_time {
                        match now.duration_since(last) {
                            Ok(passed) => {
                                if passed.as_secs() >= every_n_seconds {
                                    verbose_print();
                                    last_lai_time = Some(now);
                                }
                            }
                            Err(_) => {
                                eprintln!(
                                    "error calculating duration, erroneous clock? \
                                     last: {last:?} vs. now {now:?}"
                                );
                                // Set it in hopes of it recovering
                                last_lai_time = Some(now);
                            }
                        }
                    } else {
                        // Have to set it if it was never set or it
                        // will remain off forever.
                        last_lai_time = Some(now);
                    }
                }
            }
            sleep(Duration::from_secs_f64(sleep_seconds));
            iteration_count += 1;
        }
    }
}
