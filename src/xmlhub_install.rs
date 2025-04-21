use anyhow::{anyhow, bail, Context, Result};

use crate::{
    installation::{install::install_executable, shell::AppendToShellFileDone},
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
    let action = install_executable(&own_path)?;

    let action_bullet_points = action.show_bullet_points();
    if confirm {
        println!("Will:\n{action_bullet_points}");
        if !ask_yn("Do you want to run the above effects?")? {
            bail!("action aborted by user")
        }
    }

    let AppendToShellFileDone { provided: _ } = action.run(())?;
    println!("Successfully installed the executable.");
    if !confirm {
        println!("Did:\n\n{action_bullet_points}");
    }
    Ok(())
}
