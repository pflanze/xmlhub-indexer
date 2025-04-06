use std::{fs::File, os::fd::FromRawFd, path::Path};

use nix::{
    fcntl::{open, OFlag},
    sys::stat::Mode,
};

/// Call POSIX `open`, but wrap the resulting `fd` in
/// `std::fs::File`. Example:
///
///     let flags = OFlag::O_CREAT | OFlag::O_WRONLY | OFlag::O_TRUNC;
///     let mode: Mode = Mode::from_bits(bits_u16.into())
///         .expect("statically defined valid permission bits");
///     let file = posix_open(path, flags, mode)?;
///
pub fn posix_open<P: AsRef<Path>>(
    path: P,
    oflag: OFlag,
    mode: Mode,
) -> Result<File, std::io::Error> {
    let fd = open(path.as_ref(), oflag, mode)?;
    Ok(unsafe { File::from_raw_fd(fd) })
}
