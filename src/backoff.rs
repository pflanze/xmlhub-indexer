//! Run a job in an interval that is increased if there are errors,
//! and lowered again if there aren't.

//! Todo: measure time and subtract the job running time from the
//! sleep time? Or at least, when gettng an error, take the passed
//! time as the basis, not the `min_sleep_seconds` time (since that
//! might be 0, even).

use std::{fmt::Display, thread::sleep, time::Duration};

/// Configuration data; implements `Default` so you can initialize an
/// instance setting only the fields that you want to change (most
/// likely only the `*_sleep_seconds` values).
pub struct LoopWithBackoff {
    /// Whether to enable additional diagnostic messages to stderr
    /// (default: false).
    pub verbose: bool,
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
            verbose: false,
            quiet: false,
            error_sleep_factor: 1.05,
            success_sleep_factor: 0.99,
            min_sleep_seconds: 1.,
            max_sleep_seconds: 1000.,
        }
    }
}

impl LoopWithBackoff {
    /// Loop forever running `job` then sleeping at least `min_seconds`,
    /// if `job` returns an `Err`, increases the sleep time
    pub fn run<E: Display>(&self, mut job: impl FnMut() -> Result<(), E>) -> ! {
        let mut sleep_seconds = self.min_sleep_seconds;
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
            if self.verbose {
                eprintln!("sleeping {sleep_seconds} seconds");
            }
            sleep(Duration::from_secs_f64(sleep_seconds));
        }
    }
}
