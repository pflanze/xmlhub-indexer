use std::fmt::Display;

use anyhow::{bail, Result};

use crate::{
    backoff::LoopWithBackoff,
    unix::{easy_fork, waitpid_until_gone, Status},
};

/// Runs `job` repeatedly forever by forking off a child, running the
/// `job` in the child once. In the parent, wait for the child to end,
/// then treat both error returns and crashes / non-0 exits as errors.
/// Restart but back off before retrying, using the given
/// `LoopWithBackoff` config. Runs `until` after every run, and
/// returns if it returns true.
///
/// Note: must be run while there are no running threads, panics
/// otherwise!
pub fn forking_loop<E: Display, F: FnOnce() -> Result<(), E>>(
    config: LoopWithBackoff,
    job: F,
    until: impl Fn() -> bool,
) where
    anyhow::Error: From<E>,
{
    let mut perhaps_job = Some(job);
    config.run(
        || -> Result<()> {
            if let Some(pid) = easy_fork()? {
                // Parent process

                // XXX todo: optionally set up a thread that kills the
                // pid after a timeout.

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
                // Detach from the parent, type system wise at least.
                let mut perhaps_job = unsafe {
                    // Safety: Uh, not very sure at all. It's as if
                    // each child created the same state from scratch,
                    // with everything attached. There is no safe
                    // shared memory, so no such problems (there are
                    // *some* safe(?) shared unix resources across
                    // fork, though, like flock, XX todo.).
                    // Check:
                    // https://users.rust-lang.org/t/moving-borrowed-values-into-a-forked-child/28183
                    ((&mut perhaps_job) as *const Option<F>).read()
                };
                let job = perhaps_job.take().expect("only once per child");

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
