use anyhow::{anyhow, bail, Result};
use nix::{
    sys::signal::{kill, Signal},
    unistd::{getsid, Pid},
};

use crate::processes::Processes;

pub fn send_signal_to_pid(pid: Pid, signal: Option<Signal>) -> anyhow::Result<bool> {
    match kill(pid, signal) {
        Ok(()) => Ok(true),
        Err(e) => match e {
            nix::errno::Errno::EPERM => {
                // Can't happen because there's "no way"
                // that between us checking is_running()
                // and reading the pid and signalling
                // another process group would be there
                // than ours.  XX except, what if a member
                // of the process group exec's a setuid
                // binary?
                bail!("don't have permission to send signal to pid {pid}")
            }
            nix::errno::Errno::ESRCH => {
                // Process does not exist
                Ok(false)
            }
            _ => unreachable!(),
        },
    }
}

pub fn send_signal_to_process_group(pid: Pid, signal: Option<Signal>) -> Result<bool> {
    let process_group_id: i32 = pid
        .as_raw()
        .checked_neg()
        .ok_or_else(|| anyhow!("pid {pid} can't be negated"))?;
    send_signal_to_pid(Pid::from_raw(process_group_id), signal)
}

pub fn send_signal_to_all_processes_of_session(
    session_pid: Pid,
    signal: Option<Signal>,
) -> anyhow::Result<bool> {
    // Kill the whole process group (but that doesn't include
    // everything in the session)
    let mut reached_a_process = send_signal_to_process_group(session_pid, signal)?;

    // Iterate all processes to find ones in the given session
    let pids = Processes::default().get_list()?;
    for pid in pids {
        if let Ok(sid) = getsid(Some(pid)) {
            if sid == session_pid {
                if send_signal_to_pid(pid, signal)? {
                    reached_a_process = true;
                }
            }
        }
    }
    Ok(reached_a_process)
}
