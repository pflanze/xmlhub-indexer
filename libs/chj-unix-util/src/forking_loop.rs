use std::fmt::Display;

use anyhow::{bail, Result};

use crate::{
    backoff::LoopWithBackoff,
    unix::{easy_fork, waitpid_until_gone, Status},
};

/// Runs `job` repeatedly forever by forking off a child, running the
/// `job` in the child once, in the parent wait for the child and
/// treat both error returns and crashes / non-0 exits as errors that
/// make it back off before retrying, using the given
/// `LoopWithBackoff` config. Runs `until` after every run, and
/// returns if it returns true.
///
/// Note: must be run while there are no running threads, panics
/// otherwise!
pub fn forking_loop<E: Display>(
    config: LoopWithBackoff,
    // TODO: job could really be an FnOnce, since it's the last thing
    // running in the child before exit. But the type system doesn't
    // know about fork. How to persuade it?
    job: impl Fn() -> Result<(), E>,
    until: impl Fn() -> bool,
) where
    anyhow::Error: From<E>,
{
    config.run(
        || -> Result<()> {
            if let Some(pid) = easy_fork()? {
                // Parent process

                // XXX todo: set up a thread that kills the pid after a timeout.

                match waitpid_until_gone(pid)? {
                    Status::Normalexit(code) => {
                        if code != 0 {
                            bail!("child {pid} exited with exit code {code}");
                        }
                    }
                    Status::Signalexit(signal) => {
                        bail!("child {pid} terminated by signal {signal}");
                    }
                }
            } else {
                // Child process
                match job() {
                    Ok(()) => std::process::exit(0),
                    Err(e) => {
                        eprintln!("Error: {e:#}");
                        std::process::exit(1);
                    }
                }
            }
            Ok(())
        },
        until,
    )
}
