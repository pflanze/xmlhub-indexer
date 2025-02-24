use std::{
    fmt::Debug,
    fs::{File, OpenOptions},
    path::Path,
};

use anyhow::{anyhow, Context};

/// Open a file for reading and writing, without truncating it if it
/// exists, but creating it if it doesn't exist. The filehandle is in
/// append mode (XX is it?, only on Unix?). You can use this to open a
/// file that you intend to mutate, but need to flock first
/// (i.e. can't truncate before you've got the lock).
pub fn open_rw<P: AsRef<Path> + Debug>(path: P) -> anyhow::Result<File> {
    // Can`t use `File::create` since that
    // truncates before we have the lock.
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .append(true)
        .open(path.as_ref())
        .with_context(|| anyhow!("opening {path:?} for updating"))
}

/// Open a file for writing in append mode, without truncating it if it
/// exists, but creating it if it doesn't exist. E.g. for writing logs.
pub fn open_append<P: AsRef<Path> + Debug>(path: P) -> anyhow::Result<File> {
    // Can`t use `File::create` since that
    // truncates before we have the lock.
    OpenOptions::new()
        .write(true)
        .create(true)
        .append(true)
        .open(path.as_ref())
        .with_context(|| anyhow!("opening {path:?} for appending"))
}
