use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::{path_util::add_extension, sha256::sha256sum};

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
    const PERMS: u16 = 0o644;
    const EXCLUSIVE: bool = false;
}

impl AppInfo {
    const SUFFIX: &str = "info";

    pub fn info_path_for_app_path<P: AsRef<Path>>(path: P) -> Result<PathBuf> {
        let mut path = path.as_ref().to_owned();
        if !add_extension(&mut path, Self::SUFFIX) {
            let path: &Path = path.as_ref();
            bail!("path does not have a file name: {path:?}")
        }
        Ok(path)
    }

    /// Load the info file for the given app executable file. Also
    /// returns the path to the info file and the bytes of the content since the info file is likely
    /// to be verified with a signature. (XX: change API to only give
    /// the info file if the signature is valid?)
    pub fn load_for_app_path<P: AsRef<Path>>(
        executable_path: P,
    ) -> Result<(Self, PathBuf, Vec<u8>)> {
        let info_path = Self::info_path_for_app_path(executable_path)?;
        let content = std::fs::read(&info_path)
            .with_context(|| anyhow!("reading app info file {info_path:?}"))?;
        let content_bytes: &[u8] = &content;
        let slf = Self::from_reader(content_bytes)
            .with_context(|| anyhow!("loading from path {info_path:?}"))?;
        Ok((slf, info_path, content))
    }

    /// Save the info file for the given app executable file
    pub fn save_for_app_path<P: AsRef<Path>>(&self, executable_path: P) -> Result<PathBuf> {
        let info_path = Self::info_path_for_app_path(executable_path)?;
        self.save(&info_path)
            .with_context(|| anyhow!("saving to path {info_path:?}"))?;
        Ok(info_path)
    }

    /// Returns an error if the contents of the file at `path` does
    /// not match the expected hash. Returns the file path if OK.
    pub fn verify_binary<P: AsRef<Path>>(&self, path: P) -> Result<P> {
        let effective_hash = sha256sum(path.as_ref())?;

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
