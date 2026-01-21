use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{anyhow, bail, Context, Result};
use nix::unistd::Pid;

#[derive(Debug, Clone)]
pub enum ProcessSelection {
    All,
    User(Cow<'static, str>),
}

impl Default for ProcessSelection {
    fn default() -> Self {
        ProcessSelection::All
    }
}

#[derive(Default)]
pub struct Processes {
    pub selection: ProcessSelection,
    pub ps_path: Option<PathBuf>,
}

impl Processes {
    pub fn get_list(&self) -> Result<Vec<Pid>> {
        let Self { selection, ps_path } = self;
        let ps: &Path = "ps".as_ref();
        let ps_path: &Path = ps_path
            .as_ref()
            .map(|p| -> &Path { p.as_ref() })
            .unwrap_or(ps);
        let mut cmd = Command::new(ps_path);
        match selection {
            ProcessSelection::All => {
                cmd.args(["-e", "-o", "pid="]);
            }
            ProcessSelection::User(cow) => {
                cmd.args(["-u", cow.as_ref(), "-o", "pid="]);
            }
        }
        let output = cmd
            .output()
            .with_context(|| anyhow!("running {ps_path:?}"))?;

        if output.status.success() {
            let s = std::str::from_utf8(&output.stdout)
                .with_context(|| anyhow!("parsing output from {ps_path:?} as utf8"))?;
            let mut vec = Vec::new();
            for line in s.split("\n") {
                let s = line.trim();
                if s.is_empty() {
                    continue;
                }
                let pid: i32 = s
                    .parse()
                    .with_context(|| anyhow!("parsing {s:?} from {ps_path:?} as an i32"))?;
                if pid < 0 {
                    bail!("{ps_path:?} gave a negative number: {s:?}")
                } else {
                    vec.push(Pid::from_raw(pid));
                }
            }
            Ok(vec)
        } else {
            bail!("command {ps_path:?} did not exit successfully")
        }
    }
}
