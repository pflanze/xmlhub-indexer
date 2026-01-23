//! Trait and helpers to create restart need checkers for
//! `Daemon.other_restart_checks`.

use std::{
    io::{stderr, ErrorKind},
    ops::Deref,
    time::SystemTime,
};

use anyhow::Result;

use crate::{
    eval_with_default::EvalWithDefault, re_exec::current_exe,
    timestamp_formatter::TimestampFormatter,
};

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
        timestamp_formatter: TimestampFormatter,
    ) -> Result<RestartForExecutableChange> {
        let restart_on_upgrades = self.eval_with_default(default_want_restarting);
        let (do_check, opt_binary_mtime) = if restart_on_upgrades {
            // Consciously *not* using the 'fixed' `current_exe`: it's
            // *good* if we get the mangled path because our real path
            // is gone, so that we don't use the wrong mtime.
            let path = std::env::current_exe()?;
            match path.metadata() {
                Ok(m) => {
                    if let Ok(mtime) = m.modified() {
                        (true, Some(mtime))
                    } else {
                        (false, None)
                    }
                }
                Err(e) => {
                    let restart_if_path_shows_up_later = e.kind() == ErrorKind::NotFound;
                    (restart_if_path_shows_up_later, None)
                }
            }
        } else {
            (false, None)
        };
        Ok(RestartForExecutableChange {
            timestamp_formatter,
            do_check,
            opt_binary_mtime,
        })
    }
}

#[derive(Debug, Clone)]
pub struct RestartForExecutableChange {
    timestamp_formatter: TimestampFormatter,
    do_check: bool,
    opt_binary_mtime: Option<SystemTime>,
}

impl WarrantsRestart for &RestartForExecutableChange {
    fn warrants_restart(&self) -> bool {
        self.do_check && {
            match current_exe() {
                Ok(binary) => {
                    if let Ok(metadata) = binary.metadata() {
                        if let Ok(new_mtime) = metadata.modified() {
                            let old_mtime = self.opt_binary_mtime;
                            // if new_mtime > *mtime {  ? or allow downgrades, too:
                            if Some(new_mtime) != old_mtime {
                                use std::io::Write;
                                let tmp;
                                _ = writeln!(
                                    &mut stderr(),
                                    "the binary at {binary:?} has updated, from \
                                     {} to {}, going to restart",
                                    if let Some(old_mtime) = old_mtime {
                                        tmp = self.timestamp_formatter.format_systemtime(old_mtime);
                                        &tmp
                                    } else {
                                        "(none)"
                                    },
                                    self.timestamp_formatter.format_systemtime(new_mtime)
                                );
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
                }
                Err(_) => false,
            }
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
        let restart_for_config_change = if restart_for_config_change_opts
            .eval_with_default(default_restart_for_config_change)
        {
            Some(restart_for_config_change)
        } else {
            None
        };

        RestartForExecutableOrConfigChange {
            restart_for_executable_change,
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
    restart_for_config_change: Option<RestartForConfigChange>,
}

impl<RestartForConfigChange: Deref<Target: WarrantsRestart>> WarrantsRestart
    for RestartForExecutableOrConfigChange<RestartForConfigChange>
{
    fn warrants_restart(&self) -> bool {
        (&self.restart_for_executable_change).warrants_restart()
            || if let Some(restart_for_config_change) = &self.restart_for_config_change {
                if restart_for_config_change.warrants_restart() {
                    use std::io::Write;
                    _ = writeln!(
                        &mut stderr(),
                        "the configuration has updated, going to restart",
                    );
                    true
                } else {
                    false
                }
            } else {
                false
            }
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
