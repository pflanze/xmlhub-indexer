use anyhow::{anyhow, Context, Result};

use crate::{
    installation::{
        defaults::global_app_state_dir,
        git_based_upgrade::{
            carry_out_install_action_with_log, InstallAction, InstallActionWithLog,
        },
    },
    xmlhub_global_opts::PROGRAM_NAME,
};

#[derive(clap::Parser, Debug, Clone)]
pub struct InstallOpts {
    /// Show what is going to be done and ask for confirmation
    #[clap(long)]
    confirm: bool,
}

/// Execute an `install` command
pub fn install_command(command_opts: InstallOpts) -> Result<()> {
    let InstallOpts { confirm } = command_opts;

    let own_path = std::env::current_exe()
        .with_context(|| anyhow!("getting the path to the running executable"))?;

    carry_out_install_action_with_log(InstallActionWithLog {
        install_action: InstallAction {
            binary_path: &own_path,
            changelog_output: "",
            confirm,
            action_verb_in_past_tense: "installed",
            program_name: PROGRAM_NAME,
        },
        upgrades_log_base: &global_app_state_dir()?.upgrades_log_base()?,
        app_info: None,
    })
}
