use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use crate::util::sha256sum;

use super::json_file::{JsonFile, JsonFileHeader};

const APP_INFO_VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Debug)]
pub struct AppInfoHeader {
    app_info_version: u32,
}

impl JsonFileHeader for AppInfoHeader {
    type VersionAndKind = ();

    fn check_version_and_kind(
        &self,
        _version_and_kind: &Self::VersionAndKind,
    ) -> anyhow::Result<()> {
        if self.app_info_version != APP_INFO_VERSION {
            bail!(
                "incompatible app info file format version, expected {}, got {}",
                APP_INFO_VERSION,
                self.app_info_version
            )
        }
        Ok(())
    }

    fn new_with_version_and_kind(_version_and_kind: &Self::VersionAndKind) -> Self {
        Self {
            app_info_version: APP_INFO_VERSION,
        }
    }
}

/// Info file on application binaries, stored as
/// `binaryname.info`. Contain the application version, and hash over
/// the binary. These are themselves signed with signature file stored
/// at `binaryname.info.sig`.
#[derive(Serialize, Deserialize, Debug)]
pub struct AppInfo {
    /// Hexadecimal SHA256 hash string over the binary
    pub sha256: String,
    /// Version number:
    pub version: String,
    /// Source commit ID hash
    pub source_commit: String,
    /// Version of the compiler that produced the binary
    pub rustc_version: String,
    /// Version of cargo producing the binary
    pub cargo_version: String,
    /// OS version where the binary was built
    pub os_version: String,
    /// `username@hostname` that built the binary (probably same as in
    /// the .sig file)
    pub creator: String,
    /// Time of creation of the binary, in rfc2822 format.
    pub build_date: String,
}

impl JsonFile for AppInfo {
    type Header = AppInfoHeader;
    const VERSION_AND_KIND: () = ();
    const PERMS: u16 = 0644;
}

impl AppInfo {
    /// Returns an error if the contents of the file at `path` does
    /// not match the expected hash. Returns the file path if OK.
    pub fn verify_binary<P: AsRef<Path>>(&self, path: P) -> Result<P> {
        let effective_hash = sha256sum(&PathBuf::from("."), path.as_ref())?;

        if effective_hash != self.sha256 {
            bail!(
                "file does not match expected content: expected hash {:?}, but got {:?}",
                self.sha256,
                effective_hash
            )
        }

        Ok(path)
    }
}
