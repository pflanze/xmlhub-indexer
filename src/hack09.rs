use std::{
    os::unix::fs::symlink,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Context, Result};
use cj_path_util::path_util::AppendToPath;

use crate::{unix_passwd::getpwuid_home, utillib::home::home_dir};

/// Just adds error wrapper that mentions the path.
pub fn canonicalize(path: &Path) -> Result<PathBuf> {
    path.canonicalize()
        .with_context(|| anyhow!("canonicalizing {path:?}"))
}

/// Hack for stadler09 server: we run on a local file system
/// (`/local0/$USER/`), because `$HOME` is on a mounted file
/// share. That file share is not very reliable and additionally, is
/// unmounted when closing the last ssh connection (for a while). We
/// set `$HOME` to that local file system, but ssh ignores that
/// variable. So we also check if `~/.ssh` (in the original home) is
/// around, if not, create a symlink to `$HOME/.ssh`. `sshd` ignores
/// that symlink so this doesn not help to log into the account while
/// the share is unmounted, but the `ssh` tool is all we care about
/// (via git) and that works. Git iself does not need special
/// treatment.
pub fn hack09() -> Result<()> {
    let home_from_env_var = canonicalize(home_dir()?)?;
    let uid = nix::unistd::getuid();
    let home_from_user_database =
        canonicalize(&getpwuid_home(uid)?.ok_or_else(|| anyhow!("can't get user for uid {uid}"))?)?;
    if home_from_env_var == home_from_user_database {
        return Ok(());
    }
    // HOME is set to another home dir, check if the original ~/.ssh
    // exists, otherwise make a symlink
    let orig_ssh_path = home_from_user_database.append(".ssh");
    if let Ok(_) = orig_ssh_path.symlink_metadata() {
        return Ok(());
    }
    let target = home_from_env_var.append(".ssh");
    eprintln!("hack09: creating symlink {orig_ssh_path:?} -> {target:?}");
    symlink(&target, &orig_ssh_path)
        .with_context(|| anyhow!("symlink({target:?}, {orig_ssh_path:?})"))?;
    Ok(())
}
