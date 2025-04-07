use std::{
    fmt::Display,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, Context, Result};
use fips205::{
    slh_dsa_shake_128s,
    traits::{SerDes, Signer, Verifier},
};
use serde::{Deserialize, Serialize};

use crate::{
    path_util::add_extension,
    utillib::hex::{decode_hex, to_hex_string},
};

use super::{
    json_file::{JsonFile, JsonFileHeader},
    util::{get_creator, get_timestamp},
};

const APP_SIGNATURE_KEY_VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Copy)]
pub enum AppSignatureFileKind {
    PublicKey,
    PrivateKey,
    // PublicAndPrivateKeyPair, // ever used?
    Signature,
}

// just proxy Display to Debug, sigh
impl Display for AppSignatureFileKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{:?}", self))
    }
}

#[derive(Serialize, Deserialize)]
pub struct AppSignatureKeyHeader {
    pub app_signature_key_version: u32,
    pub kind: AppSignatureFileKind,
}

impl JsonFileHeader for AppSignatureKeyHeader {
    type VersionAndKind = AppSignatureFileKind;

    fn check_version_and_kind(&self, kind: &Self::VersionAndKind) -> Result<()> {
        if self.app_signature_key_version != APP_SIGNATURE_KEY_VERSION {
            bail!(
                "incompatible file format version, expected {}, got {}",
                APP_SIGNATURE_KEY_VERSION,
                self.app_signature_key_version
            )
        }
        if self.kind != *kind {
            bail!("invalid file kind, expected {}, got {}", kind, self.kind)
        }
        Ok(())
    }

    fn new_with_version_and_kind(kind: &Self::VersionAndKind) -> Self {
        AppSignatureKeyHeader {
            app_signature_key_version: APP_SIGNATURE_KEY_VERSION,
            kind: kind.clone(),
        }
    }
}

pub trait SaveLoadKeyFile: Serialize + JsonFile<Header = AppSignatureKeyHeader> {
    const SUFFIX: &'static str;

    fn path_add_suffix<P: AsRef<Path>>(path_without_suffix: P) -> Result<PathBuf> {
        let mut path = path_without_suffix.as_ref().to_owned();
        if !add_extension(&mut path, Self::SUFFIX) {
            bail!(
                "cannot add extension to path {:?} (does not have a file name)",
                path_without_suffix.as_ref()
            );
        }
        Ok(path)
    }

    fn save_to_base<P: AsRef<Path>>(&self, path_without_suffix: P) -> Result<()> {
        let path = Self::path_add_suffix(path_without_suffix)?;
        self.save(&path)
            .with_context(|| anyhow!("writing to file {path:?}"))
    }

    fn load_from_base<P: AsRef<Path>>(path_without_suffix: P) -> Result<Self> {
        let path = Self::path_add_suffix(path_without_suffix)?;
        JsonFile::load(&path).with_context(|| anyhow!("reading from file {path:?}"))
    }
}

pub trait DecodeKey {
    type KeyType: fips205::traits::SerDes;
    const KEY_LEN: usize;

    // Can't share implementation due to `[0; Self::KEY_LEN]` not
    // being possible, thus all there remains is the prototype.
    fn decode_key(&self) -> Result<Self::KeyType>;
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FileMetadata {
    /// Fullname < email address > (specified manually, then copied
    /// from the key file)
    pub owner: String,
    /// username@hostname (records place where this file was created)
    pub creator: String,
    /// Time of creation of this file, in rfc2822 format.
    pub birth: String,
}

#[derive(Serialize, Deserialize)]
pub struct AppSignaturePublicKey {
    pub metadata: FileMetadata,
    pub public_key: String,
}

impl JsonFile for AppSignaturePublicKey {
    type Header = AppSignatureKeyHeader;
    const VERSION_AND_KIND: AppSignatureFileKind = AppSignatureFileKind::PublicKey;
    const PERMS: u16 = 0o0444;
    const EXCLUSIVE: bool = true;
}

impl SaveLoadKeyFile for AppSignaturePublicKey {
    const SUFFIX: &'static str = "pub";
}

impl From<(FileMetadata, slh_dsa_shake_128s::PublicKey)> for AppSignaturePublicKey {
    fn from((metadata, key): (FileMetadata, slh_dsa_shake_128s::PublicKey)) -> Self {
        let public_key = to_hex_string(&key.into_bytes());
        Self {
            metadata,
            public_key,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct AppSignaturePrivateKey {
    pub metadata: FileMetadata,
    pub private_key: String,
}

impl JsonFile for AppSignaturePrivateKey {
    type Header = AppSignatureKeyHeader;
    const VERSION_AND_KIND: AppSignatureFileKind = AppSignatureFileKind::PrivateKey;
    const PERMS: u16 = 0o0400;
    const EXCLUSIVE: bool = true;
}

impl SaveLoadKeyFile for AppSignaturePrivateKey {
    const SUFFIX: &'static str = "priv";
}

impl From<(FileMetadata, slh_dsa_shake_128s::PrivateKey)> for AppSignaturePrivateKey {
    fn from((metadata, key): (FileMetadata, slh_dsa_shake_128s::PrivateKey)) -> Self {
        let private_key = to_hex_string(&key.into_bytes());
        Self {
            metadata,
            private_key,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct AppSignature {
    pub metadata: FileMetadata,
    /// As kind of a fingerprint, it's only 256 bits (before
    /// hex-encoding) anyway.
    pub public_key: String,
    pub signature: String,
}

impl JsonFile for AppSignature {
    type Header = AppSignatureKeyHeader;
    const VERSION_AND_KIND: AppSignatureFileKind = AppSignatureFileKind::Signature;
    const PERMS: u16 = 0o0444;
    const EXCLUSIVE: bool = true;
}

impl SaveLoadKeyFile for AppSignature {
    const SUFFIX: &'static str = "sig";
}

// -----------------------------------------------------------------------------

impl DecodeKey for AppSignaturePublicKey {
    type KeyType = slh_dsa_shake_128s::PublicKey;
    const KEY_LEN: usize = slh_dsa_shake_128s::PK_LEN;

    fn decode_key(&self) -> Result<Self::KeyType> {
        let mut bytes = [0; Self::KEY_LEN];
        decode_hex(self.public_key.as_bytes(), &mut bytes)?;
        Ok(Self::KeyType::try_from_bytes(&bytes)
            .map_err(|e| anyhow!("recreating public key from data: {e}"))?)
    }
}

impl DecodeKey for AppSignaturePrivateKey {
    type KeyType = slh_dsa_shake_128s::PrivateKey;
    const KEY_LEN: usize = slh_dsa_shake_128s::SK_LEN;

    fn decode_key(&self) -> Result<Self::KeyType> {
        let mut bytes = [0; Self::KEY_LEN];
        decode_hex(self.private_key.as_bytes(), &mut bytes)?;
        Ok(Self::KeyType::try_from_bytes(&bytes)
            .map_err(|e| anyhow!("recreating private key from data: {e}"))?)
    }
}

// Can't `impl DecodeKey for AppSignature` because `type KeyType =
// [u8; Self::KEY_LEN]` does not impl `SerDes`. And now impl DecodeKey
// for the embedded public_key (below)!
impl AppSignature {
    const SIG_LEN: usize = slh_dsa_shake_128s::SIG_LEN;

    fn decode(&self) -> Result<[u8; Self::SIG_LEN]> {
        let mut bytes = [0; Self::SIG_LEN];
        decode_hex(self.signature.as_bytes(), &mut bytes)?;
        Ok(bytes)
    }
}

impl DecodeKey for AppSignature {
    type KeyType = slh_dsa_shake_128s::PublicKey;
    const KEY_LEN: usize = slh_dsa_shake_128s::PK_LEN;

    fn decode_key(&self) -> Result<Self::KeyType> {
        let mut bytes = [0; Self::KEY_LEN];
        decode_hex(self.public_key.as_bytes(), &mut bytes)?;
        Ok(Self::KeyType::try_from_bytes(&bytes)
            .map_err(|e| anyhow!("recreating public key from data: {e}"))?)
    }
}

impl AppSignaturePrivateKey {
    pub fn sign(&self, content: &[u8]) -> Result<AppSignature> {
        let key = self.decode_key()?;
        let public_key = to_hex_string(&key.get_public_key().into_bytes());
        // hedged = ?
        let signature = key
            .try_sign(content, &[], true)
            .map_err(|e: &str| anyhow!("signing data with private key: {e}"))?;
        let signature = to_hex_string(&signature);
        Ok(AppSignature {
            metadata: FileMetadata {
                owner: self.metadata.owner.clone(),
                creator: get_creator()?,
                birth: get_timestamp(),
            },
            public_key,
            signature,
        })
    }
}

impl AppSignature {
    /// Returns whether the signature is valid, and the public key
    /// (for verification whether the key is trusted)
    pub fn verify(&self, content: &[u8]) -> Result<(bool, String)> {
        let key = self.decode_key()?;
        let sig = self.decode()?;
        let is_valid = key.verify(content, &sig, &[]);
        Ok((is_valid, self.public_key.clone()))
    }
}

impl AppSignaturePublicKey {
    /// Returns whether the signature is valid, and the public key
    /// (for verification whether the key is trusted)
    pub fn verify(&self, content: &[u8], signature: &AppSignature) -> Result<(bool, String)> {
        if self.public_key != signature.public_key {
            bail!(
                "signature created with key {:?}, but expected {:?}",
                signature.public_key,
                self.public_key
            )
        }
        signature.verify(content)
    }
}

// -----------------------------------------------------------------------------
// Utility class, not serializable directly. Also quite pointless
// since the private key contains the public key anyway, huh.

pub struct AppSignatureKeyPair {
    pub metadata: FileMetadata,
    pub public_key: slh_dsa_shake_128s::PublicKey,
    pub private_key: slh_dsa_shake_128s::PrivateKey,
}

impl AppSignatureKeyPair {
    pub fn split(self) -> (AppSignaturePublicKey, AppSignaturePrivateKey) {
        let Self {
            metadata,
            public_key,
            private_key,
        } = self;
        (
            (metadata.clone(), public_key).into(),
            (metadata, private_key).into(),
        )
    }
}

impl AppSignatureKeyPair {
    pub fn try_keygen(owner: String) -> Result<Self> {
        let metadata = FileMetadata {
            owner,
            creator: get_creator()?,
            birth: get_timestamp(),
        };
        let (public_key, private_key) = slh_dsa_shake_128s::try_keygen()
            .map_err(|e| anyhow!("calling try_keygen in fips205 library: {e}"))?;
        Ok(Self {
            metadata,
            public_key,
            private_key,
        })
    }

    pub fn save_to_base<P: AsRef<Path>>(self, path_without_suffix: P) -> Result<()> {
        let (public_key, private_key) = self.split();
        public_key.save_to_base(path_without_suffix.as_ref())?;
        private_key.save_to_base(path_without_suffix.as_ref())?;
        Ok(())
    }
}
