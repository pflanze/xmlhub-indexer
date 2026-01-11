//! Some utilities for unix specific functionality

use std::fs::File;
use std::ops::Deref;
use std::ops::DerefMut;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd};

use nix::fcntl::{flock, FlockArg};
use nix::sys::wait::{waitpid, WaitStatus};
use nix::{
    errno::Errno,
    sys::signal::Signal,
    unistd::{fork, ForkResult, Pid},
};
use num_threads::is_single_threaded;

// Don't make it overly complicated, please. The original API is
// simple enough. If a Pid is given, it's the parent.
//
/// This function can only be run if there are no other threads
/// running; it checks and panics if there are!
pub fn easy_fork() -> Result<Option<Pid>, Errno> {
    if let Some(single) = is_single_threaded() {
        if !single {
            panic!("easy_fork: other threads are running, refusing to fork")
        }
    } else {
        panic!("easy_fork: can't determine if other threads are running")
    }
    match unsafe {
        // Safe because there are no other threads (we checked above).
        fork()
    }? {
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

/// Represents an active lock via `flock`. Dropping it releases the
/// lock.
pub struct FlockGuard<'t> {
    file: Option<&'t mut File>,
}

impl<'t> FlockGuard<'t> {
    /// This "leaks" the lock, i.e. there will be no unlocking done on
    /// Drop. This is necessary if you fork and either parent and
    /// child should not release the lock for both processes. No
    /// leaking of memory is happening.
    pub fn leak(&mut self) -> Option<&'t mut File> {
        self.file.take()
    }
}

impl<'t> Deref for FlockGuard<'t> {
    type Target = File;

    fn deref(&self) -> &Self::Target {
        self.file
            .as_ref()
            .expect("do not dereference the FlockGuard after calling leak() on it")
    }
}

impl<'t> DerefMut for FlockGuard<'t> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.file
            .as_mut()
            .expect("do not dereference the FlockGuard after calling leak() on it")
    }
}

impl<'t> Drop for FlockGuard<'t> {
    fn drop(&mut self) {
        if let Some(file) = &self.file {
            let bfd: BorrowedFd = file.as_fd();
            let fd: i32 = bfd.as_raw_fd();
            match flock(fd, FlockArg::Unlock) {
                Ok(()) => (),
                Err(_e) => {
                    // XX Can't do this since it could panic. Perhaps can't lock stderr either?
                    // eprintln!(
                    //     "warning: FlockGuard::drop: unexpected error releasing file lock: {e:#}"
                    // )
                }
            }
        }
    }
}

pub fn easy_flock(
    file: &mut File,
    exclusive: bool,
    nonblock: bool,
) -> Result<Option<FlockGuard<'_>>, Errno> {
    let bfd: BorrowedFd = file.as_fd();
    let fd: i32 = bfd.as_raw_fd();
    let mode = if exclusive {
        if nonblock {
            FlockArg::LockExclusiveNonblock
        } else {
            FlockArg::LockExclusive
        }
    } else {
        if nonblock {
            FlockArg::LockSharedNonblock
        } else {
            FlockArg::LockShared
        }
    };
    match flock(fd, mode) {
        Ok(()) => Ok(Some(FlockGuard { file: Some(file) })),
        Err(e) => match e {
            // Same as Errno::EAGAIN
            Errno::EWOULDBLOCK => Ok(None),
            _ => Err(e),
        },
    }
}

pub fn easy_flock_nonblocking(
    file: &mut File,
    exclusive: bool,
) -> Result<Option<FlockGuard<'_>>, Errno> {
    easy_flock(file, exclusive, true)
}

pub fn easy_flock_blocking(file: &mut File, exclusive: bool) -> Result<FlockGuard<'_>, Errno> {
    easy_flock(file, exclusive, false)
        .map(|v| v.expect("said blocking, thus always getting the lock"))
}
