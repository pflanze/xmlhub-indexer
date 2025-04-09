//! Lol, I'm doing here what the Rust devs have avoided: enums instead
//! of strings. But they need to be extensible (future compatible), I
//! don't without updating the code in lockstep. Thus for me it's better this way.

use std::{path::PathBuf, str::FromStr};

use anyhow::{bail, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    MacOS,
    Linux,
}

impl FromStr for Os {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "macos" => Ok(Os::MacOS),
            "linux" => Ok(Os::Linux),
            _ => bail!("unknown OS {s:?}"),
        }
    }
}

impl Os {
    pub fn from_local() -> Result<Self> {
        std::env::consts::OS.parse()
    }

    pub fn as_str_for_folder_names(self) -> &'static str {
        match self {
            Os::MacOS => "macOS",
            Os::Linux => "linux",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    X86_64,
    Aarch64,
}

impl FromStr for Arch {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "x86_64" => Ok(Arch::X86_64),
            "aarch64" => Ok(Arch::Aarch64),
            _ => bail!("unknown architecture {s:?}"),
        }
    }
}

impl Arch {
    pub fn from_local() -> Result<Self> {
        std::env::consts::ARCH.parse()
    }

    pub fn as_str_for_folder_names(self) -> &'static str {
        match self {
            Arch::X86_64 => "x86_64",
            Arch::Aarch64 => "aarch64",
        }
    }
}

pub struct BinariesRepoSection {
    pub os: Os,
    pub arch: Arch,
}

impl BinariesRepoSection {
    pub fn from_local_os_and_arch() -> Result<Self> {
        Ok(Self {
            os: Os::from_local()?,
            arch: Arch::from_local()?,
        })
    }

    /// Path segments to the subdir for the given OS and architecture.
    pub fn installation_subpath_segments(&self) -> Vec<&'static str> {
        vec![
            self.os.as_str_for_folder_names(),
            self.arch.as_str_for_folder_names(),
        ]
    }

    pub fn push_installation_subpath_onto(&self, path: &mut PathBuf) {
        for segment in self.installation_subpath_segments() {
            path.push(segment)
        }
    }

    /// Relative path to the subdir for the given OS and architecture.
    pub fn installation_subpath(&self) -> PathBuf {
        let mut res = PathBuf::new();
        self.push_installation_subpath_onto(&mut res);
        res
    }
}
