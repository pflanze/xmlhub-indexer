//! Some utilities for unix specific functionality

use nix::sys::wait::{waitpid, WaitStatus};
use nix::{
    errno::Errno,
    sys::signal::Signal,
    unistd::{fork, ForkResult, Pid},
};

// Don't make it overly complicated, please. The original API is
// simple enough. If a Pid is given, it's the parent.
//
// Do not swallow the unsafe. Fork should be safe in our usage
// though: it should be safe even with allocation in the child as:
//  - we should not be using threading in this program (libs, though?)
//  - isn't libc's malloc safe anyway with fork?
//  - and we're not (consciously) touching any other mutexes in the children.
//
pub unsafe fn easy_fork() -> Result<Option<Pid>, Errno> {
    match fork()? {
        ForkResult::Parent { child, .. } => Ok(Some(child)),
        ForkResult::Child => Ok(None),
    }
}

pub enum Status {
    Normalexit(i32),
    Signalexit(Signal),
}

// Really wait until the given process has ended,
// and return a simpler enum.
pub fn waitpid_until_gone(pid: Pid) -> Result<Status, Errno> {
    loop {
        let st = waitpid(pid, None)?;
        match st {
            WaitStatus::Exited(_pid, exitcode) => return Ok(Status::Normalexit(exitcode)),
            WaitStatus::Signaled(_pid, signal, _bool) => return Ok(Status::Signalexit(signal)),
            _ => {} // retry
        }
    }
}
