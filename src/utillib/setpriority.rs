//! nix is failing me with safe interfaces?

use std::{ffi::CStr, mem::transmute};

use anyhow::{bail, Result};
use nix::{
    errno::errno,
    libc::{self, c_int, strerror_r},
};

#[derive(Debug, Clone, Copy)]
pub enum PriorityWhich {
    Process(libc::id_t),
    ProcessGroup(libc::id_t),
    User(libc::id_t),
}

// Somehow the type `__priority_which_t` is *not* available on macOS
// with the same libc version, 0.2.153, thus alias it here (without
// the leading underscores, OK?) in guest space.

#[cfg(target_os = "linux")]
#[allow(non_camel_case_types)]
pub type priority_which_t = libc::__priority_which_t;

#[cfg(target_os = "macos")]
#[allow(non_camel_case_types)]
pub type priority_which_t = i32;

impl PriorityWhich {
    fn which(self) -> priority_which_t {
        match self {
            PriorityWhich::Process(_) => libc::PRIO_PROCESS,
            PriorityWhich::ProcessGroup(_) => libc::PRIO_PGRP,
            PriorityWhich::User(_) => libc::PRIO_USER,
        }
    }

    fn who(self) -> libc::id_t {
        match self {
            PriorityWhich::Process(v) => v,
            PriorityWhich::ProcessGroup(v) => v,
            PriorityWhich::User(v) => v,
        }
    }
}

// Huh, why does nix not have that???
pub fn strerror(errno: i32) -> String {
    const BUFLEN: usize = 1024;
    let mut msg: [i8; BUFLEN] = [0; BUFLEN];
    let msgptr: *mut i8 = msg.as_mut_ptr();
    let res = unsafe { strerror_r(errno, msgptr, BUFLEN - 1) };
    assert_eq!(res, 0);
    let msgref: &[u8; BUFLEN] = unsafe { transmute(&msg) };
    let msg =
        CStr::from_bytes_until_nul(msgref).expect("can this fail? when null byte is missing?");
    String::from_utf8_lossy(msg.to_bytes()).to_string()
}

/// Careful, seems to not be working on macOS, currently.
pub fn setpriority(which: PriorityWhich, prio: c_int) -> Result<()> {
    let res = unsafe { libc::setpriority(which.which(), which.who(), prio) };
    if res < 0 {
        let err = strerror(errno());
        bail!("setpriority({which:?}, {prio}): {err}")
    }
    Ok(())
}

/// Wrapper that never fails on macOS, but might just be doing nothing
/// there. This is a HACK to work around some problem with
/// `setpriority` now giving a runtime error.
pub fn possibly_setpriority(which: PriorityWhich, prio: c_int) -> Result<()> {
    #[cfg(target_os = "linux")]
    return setpriority(which, prio);

    #[cfg(target_os = "macos")]
    {
        let _ = setpriority(which, prio);
        return Ok(());
    }
}
