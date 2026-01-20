//! Trait and helpers to create restart need checkers for
//! `Daemon.other_restart_checks`.

use std::{ops::Deref, path::Path, sync::Arc, time::SystemTime};

use anyhow::Result;

use crate::eval_with_default::EvalWithDefault;

/// The base trait for all "other" restart checkers.
pub trait WarrantsRestart {
    /// Whether a restart is warranted given the current situation.
    fn warrants_restart(&self) -> bool;
}

#[derive(Debug, Clone, Copy, clap::Args)]
#[clap(global_setting(clap::AppSettings::DeriveDisplayOrder))]
pub struct RestartForExecutableChangeOpts {
    #[clap(long)]
    restart_on_upgrades: bool,

    /// Whether to restart the daemon when its executable is changed
    /// (upgraded). Restarts only happen at safe checkpoints. The
    /// options cancel each other out; the default is determined by
    /// the application (see help text higher up).
    #[clap(long)]
    no_restart_on_upgrades: bool,
}

impl EvalWithDefault for RestartForExecutableChangeOpts {
    fn explicit_yes_and_no(&self) -> (bool, bool) {
        let RestartForExecutableChangeOpts {
            restart_on_upgrades,
            no_restart_on_upgrades,
        } = self;
        (*restart_on_upgrades, *no_restart_on_upgrades)
    }
}

impl RestartForExecutableChangeOpts {
    pub fn to_restarter(
        &self,
        default_want_restarting: bool,
    ) -> Result<RestartForExecutableChange> {
        let restart_on_upgrades = self.eval_with_default(default_want_restarting);
        let opt_binary_and_mtime = if restart_on_upgrades {
            let path = std::env::current_exe()?;
            match path.metadata() {
                Ok(m) => {
                    if let Ok(mtime) = m.modified() {
                        Some((path.into(), mtime))
                    } else {
                        None
                    }
                }
                Err(_) => None,
            }
        } else {
            None
        };
        Ok(RestartForExecutableChange {
            opt_binary_and_mtime,
        })
    }
}

#[derive(Debug, Clone)]
pub struct RestartForExecutableChange {
    opt_binary_and_mtime: Option<(Arc<Path>, SystemTime)>,
}

impl WarrantsRestart for &RestartForExecutableChange {
    fn warrants_restart(&self) -> bool {
        if let Some((binary, mtime)) = &self.opt_binary_and_mtime {
            if let Ok(metadata) = binary.metadata() {
                if let Ok(new_mtime) = metadata.modified() {
                    // if new_mtime > *mtime {  ? or allow downgrades, too:
                    if new_mtime != *mtime {
                        // info!(
                        //     "this binary at {binary:?} has updated, \
                        //      from {} to {}, going to re-exec",
                        //     SystemTimeWithDisplay(*mtime),
                        //     SystemTimeWithDisplay(new_mtime)
                        // );
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        }
    }
}

impl RestartForExecutableChange {
    pub fn and_config_change_opts<RestartForConfigChange: Deref<Target: WarrantsRestart>>(
        self,
        restart_for_config_change_opts: RestartForConfigChangeOpts,
        default_restart_for_config_change: bool,
        restart_for_config_change: RestartForConfigChange,
    ) -> RestartForExecutableOrConfigChange<RestartForConfigChange> {
        let restart_for_executable_change = self;
        let do_restart_for_config_change =
            restart_for_config_change_opts.eval_with_default(default_restart_for_config_change);

        RestartForExecutableOrConfigChange {
            restart_for_executable_change,
            do_restart_for_config_change,
            restart_for_config_change,
        }
    }
}

// Derefence to itself. OOKAY? -- Oh, *this* one leads to infinite recursion.
// impl Deref for RestartForExecutableChange {
//     type Target = RestartForExecutableChange;

//     fn deref(&self) -> &Self::Target {
//         self
//     }
// }

#[derive(Debug, Clone, Copy, clap::Args)]
#[clap(global_setting(clap::AppSettings::DeriveDisplayOrder))]
pub struct RestartForConfigChangeOpts {
    #[clap(long)]
    restart_on_config_change: bool,

    /// Whether to restart the daemon when its configuration is
    /// changed. Restarts only happen at safe checkpoints. The options
    /// cancel each other out; the default is determined by the
    /// application (see help text higher up).
    #[clap(long)]
    no_restart_on_config_change: bool,
}

impl EvalWithDefault for RestartForConfigChangeOpts {
    fn explicit_yes_and_no(&self) -> (bool, bool) {
        let RestartForConfigChangeOpts {
            restart_on_config_change,
            no_restart_on_config_change,
        } = self;
        (*restart_on_config_change, *no_restart_on_config_change)
    }
}

#[derive(Debug, Clone)]
pub struct RestartForExecutableOrConfigChange<
    RestartForConfigChange: Deref<Target: WarrantsRestart>,
> {
    restart_for_executable_change: RestartForExecutableChange,
    do_restart_for_config_change: bool,
    restart_for_config_change: RestartForConfigChange,
}

impl<RestartForConfigChange: Deref<Target: WarrantsRestart>> WarrantsRestart
    for RestartForExecutableOrConfigChange<RestartForConfigChange>
{
    fn warrants_restart(&self) -> bool {
        (&self.restart_for_executable_change).warrants_restart()
            || (self.do_restart_for_config_change
                && self.restart_for_config_change.warrants_restart())
    }
}

// Derefence to itself. OOKAY?
impl<RestartForConfigChange: Deref<Target: WarrantsRestart>> Deref
    for RestartForExecutableOrConfigChange<RestartForConfigChange>
{
    type Target = RestartForExecutableOrConfigChange<RestartForConfigChange>;

    fn deref(&self) -> &Self::Target {
        self
    }
}

/// For when no situations (other than the daemon state, i.e. the user
/// explicitly asking for it) are ever warranting restart.
#[derive(Debug, Clone)]
pub struct NoOtherRestarts;

impl WarrantsRestart for NoOtherRestarts {
    fn warrants_restart(&self) -> bool {
        false
    }
}

// Derefence to itself. OOKAY?
impl Deref for NoOtherRestarts {
    type Target = NoOtherRestarts;

    fn deref(&self) -> &Self::Target {
        self
    }
}

// Do we even need this, for after Deref?
// impl WarrantsRestart for &NoOtherRestarts {
//     fn warrants_restart(&self) -> bool {
//         false
//     }
// }
