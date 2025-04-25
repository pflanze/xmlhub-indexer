use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
pub use nix::libc::{gid_t, uid_t};
use nix::unistd::Uid;

/// Get the home directly via the password database, ignoring the
/// `HOME` env variable. Returns `None` if the user database does not
/// exist (another lookup used?) or the user can not be found?
pub fn getpwuid_home(uid: Uid) -> Result<Option<PathBuf>> {
    let user =
        nix::unistd::User::from_uid(uid).with_context(|| anyhow!("can't get User from uid"))?;
    Ok(user.map(|user| user.dir))
}
