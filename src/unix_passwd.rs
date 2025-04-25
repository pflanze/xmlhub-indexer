use std::ffi::{c_char, CStr, CString, OsString};
use std::os::unix::prelude::OsStringExt;
use std::path::PathBuf;
use std::ptr::null_mut;

use anyhow::{anyhow, bail, Result};
pub use nix::libc::{gid_t, uid_t};
use nix::{errno::errno, libc::getpwuid_r};

use crate::utillib::setpriority::strerror;

pub struct Passwd {
    pub pw_name: Option<CString>,
    // pub pw_passwd: None,
    pub pw_uid: uid_t,
    pub pw_gid: gid_t,
    pub pw_gecos: Option<CString>,
    pub pw_dir: Option<CString>,
    pub pw_shell: Option<CString>,
}

pub fn getuid() -> uid_t {
    // "These functions are always successful and never modify errno."
    unsafe { nix::libc::getuid() }
}

// Why do I have to write this? Can't find any existing wrapper, nix
// does not appear to have it.--Oooh, TODO: check
// nix::unistd::User::from_uid
pub fn getpwuid(uid: uid_t) -> Result<Passwd> {
    const BUFLEN: usize = 8000;
    let mut buffer: [i8; BUFLEN] = [0; BUFLEN];
    // `passwd` does not impl Default, nor do pointers.
    let mut passwd = nix::libc::passwd {
        pw_name: null_mut(),
        pw_passwd: null_mut(),
        pw_uid: Default::default(),
        pw_gid: Default::default(),
        pw_gecos: null_mut(),
        pw_dir: null_mut(),
        pw_shell: null_mut(),
    };
    let mut passwd_ptr_ptr: *mut nix::libc::passwd = &mut passwd;
    let res = unsafe {
        getpwuid_r(
            uid,
            &mut passwd,
            buffer.as_mut_ptr(),
            BUFLEN,
            &mut passwd_ptr_ptr,
        )
    };
    if res < 0 {
        let err = strerror(errno());
        bail!("getpwuid_r: {err}")
    }
    if passwd_ptr_ptr.is_null() {
        let err = strerror(errno());
        bail!("getpwuid_r (returned null ptr): {err}")
    }
    fn ownify(ptr: *const c_char) -> Option<CString> {
        if ptr.is_null() {
            None
        } else {
            Some(unsafe { CStr::from_ptr(ptr) }.to_owned())
        }
    }
    let pw_name = ownify(passwd.pw_name);
    let pw_uid = passwd.pw_uid;
    let pw_gid = passwd.pw_gid;
    let pw_gecos = ownify(passwd.pw_gecos);
    let pw_dir = ownify(passwd.pw_dir);
    let pw_shell = ownify(passwd.pw_shell);
    Ok(Passwd {
        pw_name,
        pw_uid,
        pw_gid,
        pw_gecos,
        pw_dir,
        pw_shell,
    })
}

pub fn unix_cstring_to_osstring(s: CString) -> OsString {
    OsString::from_vec(s.into_bytes())
}

/// Get the home directly via the password database, ignoring the
/// `HOME` env variable.
pub fn getpwuid_home(uid: uid_t) -> Result<PathBuf> {
    let passwd = getpwuid(uid)?;
    let path: OsString = unix_cstring_to_osstring(
        passwd
            .pw_dir
            .ok_or_else(|| anyhow!("getpwuid returned no home dir for uid {uid}"))?,
    );
    Ok(path.into())
}
