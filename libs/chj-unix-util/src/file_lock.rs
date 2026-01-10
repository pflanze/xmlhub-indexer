use std::{fs::File, os::unix::fs::OpenOptionsExt, path::Path};

use nix::errno::Errno;
use ouroboros::self_referencing;

use crate::unix::{easy_flock_nonblocking, FlockGuard};

#[self_referencing]
pub struct FileLock {
    file: File,

    #[borrows(mut file)]
    #[covariant]
    flock_guard: FlockGuard<'this>,
}

#[derive(thiserror::Error, Debug)]
pub enum FileLockError {
    #[error("opening file path")]
    OpenError(#[from] std::io::Error),
    #[error("calling flock")]
    FlockError(#[from] Errno),
    #[error("lock already taken")]
    AlreadyLocked,
}

impl FileLock {
    pub fn leak(&mut self) {
        self.with_flock_guard_mut(|g| g.leak());
    }
}

/// Try to get an flock based lock on a lock file, give an
/// `FileLockError::AlreadyLocked` error if can't get it. The file is
/// created or truncated.
pub fn file_lock_nonblocking<P: AsRef<Path>>(
    path: P,
    exclusive: bool,
) -> Result<FileLock, FileLockError> {
    let mut opts = File::options();
    opts.read(true);
    opts.write(true);
    opts.truncate(false);
    opts.create(true);
    opts.mode(0o600); // XX how to make portable?
    let file = opts.open(path.as_ref())?;
    FileLock::try_new(file, |file| {
        if let Some(flock_guard) = easy_flock_nonblocking(file, exclusive)? {
            Ok(flock_guard)
        } else {
            Err(FileLockError::AlreadyLocked)
        }
    })
}
