use anyhow::{anyhow, bail, Context, Result};

use crate::{installation::install::install_executable, xmlhub_global_opts::GlobalOpts};

#[derive(clap::Parser, Debug, Clone)]
pub struct InstallOpts {}

/// Execute an `install` command
pub fn install_command(global_opts: &GlobalOpts, command_opts: InstallOpts) -> Result<()> {
    let InstallOpts {} = command_opts;

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
    let done = install_executable(&own_path)?;
    println!("Successfully installed the executable:\n\n{done}");

    Ok(())
}
