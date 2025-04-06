use std::{
    fs::File,
    io::{BufReader, BufWriter, Read, Write},
};

use anyhow::{anyhow, bail, Context, Result};
use chrono::Local;
use fips205::{
    slh_dsa_shake_128s,
    traits::{SerDes, Signer, Verifier},
};
use nix::{fcntl::OFlag, sys::stat::Mode};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{
    util::hostname,
    utillib::hex::{decode_hex, to_hex_string},
};

use super::private_file::posix_open;

/// Read individual JSON items from a stream
pub fn serde_json_reader<T: DeserializeOwned, R: Read>(
    input: R,
) -> impl Iterator<Item = Result<T, serde_json::Error>> {
    serde_json::Deserializer::from_reader(input).into_iter()
}

/// Read individual JSON items from a stream
pub fn serde_json_maybe_read1<T: DeserializeOwned, R: Read>(
    input: R,
) -> Result<Option<T>, serde_json::Error> {
    let mut iter = serde_json_reader::<T, R>(input);
    iter.next().transpose()
}

/// Read exactly one JSON item from a stream
pub fn serde_json_read1<T: DeserializeOwned, R: Read>(input: R) -> Result<T> {
    if let Some(item) = serde_json_maybe_read1::<T, R>(input)? {
        Ok(item)
    } else {
        bail!("premature EOF reading JSON item")
    }
}

/// Current time in rfc2822 format.
fn get_timestamp() -> String {
    Local::now().to_rfc2822()
}

/// Get user@hostname
fn get_creator() -> Result<String> {
    let username = std::env::var("USER").context("retrieving USER environment variable")?;
    let hostname = hostname()?;
    Ok(format!("{username}@{hostname}"))
}

const APP_SIGNATURE_KEY_VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum AppSignatureFileKind {
    PublicKey,
    PrivateKey,
    // PublicAndPrivateKeyPair, // ever used?
    Signature,
}

#[derive(Serialize, Deserialize)]
pub struct AppSignatureKeyHeader {
    pub app_signature_key_version: u32,
    pub kind: AppSignatureFileKind,
}

pub trait SaveLoadKeyFile: Serialize + DeserializeOwned + Sized + 'static {
    const SUFFIX: &'static str;
    const KIND: AppSignatureFileKind;
    const PERMS: u16;

    fn writeln<W: Write>(&self, out: &mut W) -> Result<()> {
        let header = AppSignatureKeyHeader {
            app_signature_key_version: APP_SIGNATURE_KEY_VERSION,
            kind: Self::KIND,
        };
        serde_json::to_writer(&mut *out, &header)?;
        out.write_all(b"\n")?;
        serde_json::to_writer(&mut *out, self)?;
        out.write_all(b"\n")?;
        Ok(())
    }

    fn save_to_base(&self, path_without_suffix: &str) -> Result<()> {
        let path = format!("{path_without_suffix}.{}", Self::SUFFIX);
        (|| -> Result<()> {
            // No need for O_EXCL since we make it read-only via mode
            // and thus won't accidentally overwrite it anyway, OK?
            let flags = OFlag::O_CREAT | OFlag::O_WRONLY | OFlag::O_TRUNC;
            let mode: Mode = Mode::from_bits(Self::PERMS.into())
                .expect("statically defined valid permission bits");
            let out = posix_open(&path, flags, mode)?;
            let mut out = BufWriter::new(out);
            self.writeln(&mut out)?;
            out.flush()?;
            Ok(())
        })()
        .with_context(|| anyhow!("writing to file {path:?}"))
    }

    fn from_reader<R: Read>(mut input: R) -> Result<Self> {
        let header: AppSignatureKeyHeader = serde_json_read1(&mut input)?;
        if header.app_signature_key_version != APP_SIGNATURE_KEY_VERSION {
            bail!(
                "invalid key version, expected {}, got {}",
                APP_SIGNATURE_KEY_VERSION,
                header.app_signature_key_version
            )
        }
        if header.kind != Self::KIND {
            bail!(
                "invalid key kind, expected {:?}, got {:?}",
                Self::KIND,
                header.kind
            )
        }
        // Could use serde_json_read1 again, or let it error out now
        // if more than one item left?
        Ok(serde_json::from_reader(&mut input)?)
    }

    fn load_from_base(path_without_suffix: &str) -> Result<Self> {
        let path = format!("{path_without_suffix}.{}", Self::SUFFIX);
        (|| -> Result<Self> {
            let inp = BufReader::new(File::open(&path)?);
            Ok(Self::from_reader(inp)?)
        })()
        .with_context(|| anyhow!("reading from file {path:?}"))
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

impl SaveLoadKeyFile for AppSignaturePublicKey {
    const SUFFIX: &'static str = "pub";
    const KIND: AppSignatureFileKind = AppSignatureFileKind::PublicKey;
    const PERMS: u16 = 0o0444;
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

impl SaveLoadKeyFile for AppSignaturePrivateKey {
    const SUFFIX: &'static str = "priv";
    const KIND: AppSignatureFileKind = AppSignatureFileKind::PrivateKey;
    const PERMS: u16 = 0o0400;
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

impl SaveLoadKeyFile for AppSignature {
    const SUFFIX: &'static str = "sig";
    const KIND: AppSignatureFileKind = AppSignatureFileKind::Signature;
    const PERMS: u16 = 0o0444;
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

    pub fn save_to_base(self, path_without_suffix: &str) -> Result<()> {
        let (public_key, private_key) = self.split();
        public_key.save_to_base(path_without_suffix)?;
        private_key.save_to_base(path_without_suffix)?;
        Ok(())
    }
}
