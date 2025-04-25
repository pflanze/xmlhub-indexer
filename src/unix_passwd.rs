use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
pub use nix::libc::{gid_t, uid_t};
use nix::unistd::Uid;

/// Get the home directly via the password database, ignoring the
/// `HOME` env variable.
pub fn getpwuid_home(uid: Uid) -> Result<PathBuf> {
    let user = nix::unistd::User::from_uid(uid)
        .with_context(|| anyhow!("can't get User from uid"))?
        .ok_or_else(|| anyhow!("got None getting User from uid"))?;
    Ok(user.dir)
}
