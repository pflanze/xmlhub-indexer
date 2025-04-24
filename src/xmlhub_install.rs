use anyhow::{anyhow, bail, Context, Result};

use crate::{
    installation::{
        git_based_upgrade::carry_out_install_action, install::install_executable,
        shell::AppendToShellFileDone,
    },
    util::ask_yn,
    xmlhub_global_opts::GlobalOpts,
};

#[derive(clap::Parser, Debug, Clone)]
pub struct InstallOpts {
    /// Show what is going to be done and ask for confirmation
    #[clap(long)]
    confirm: bool,
}

/// Execute an `install` command
pub fn install_command(global_opts: &GlobalOpts, command_opts: InstallOpts) -> Result<()> {
    let InstallOpts { confirm } = command_opts;

    if global_opts.dry_run {
        // XX todo?
        bail!("--dry-run is not currently supported for `install`")
    }
    if global_opts.verbose {
        // XX todo?
        bail!("--verbose is not currently supported for `install`")
    }

    let own_path = std::env::current_exe()
        .with_context(|| anyhow!("getting the path to the running executable"))?;

    carry_out_install_action(&own_path, "", confirm, "installed")
}
