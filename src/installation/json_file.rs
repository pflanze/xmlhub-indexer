//! A generic way to store typed data using JSON, with a header with
//! file type and version. A file contains (at least) two parts, the
//! header, and the actual value, both serialized as JSON, and written
//! with a newline inbetween (and currently the JSON parts are single
//! lines, although for reading that's not necessary as a streaming
//! parsing process is used that doesn't depend on that).

use std::{
    fs::File,
    io::{BufReader, BufWriter, Read, Write},
    path::Path,
};

use anyhow::Result;
use nix::{fcntl::OFlag, sys::stat::Mode};
use serde::{de::DeserializeOwned, Serialize};

use super::{private_file::posix_open, serde_json_util::serde_json_read1};

pub trait JsonFileHeader {
    type VersionAndKind;
    fn check_version_and_kind(&self, version_and_kind: &Self::VersionAndKind) -> Result<()>;
    fn new_with_version_and_kind(version_and_kind: &Self::VersionAndKind) -> Self;
}

pub trait JsonFile: Sized + Serialize + DeserializeOwned {
    type Header: JsonFileHeader + Serialize + DeserializeOwned;
    const VERSION_AND_KIND: <<Self as JsonFile>::Header as JsonFileHeader>::VersionAndKind;
    const PERMS: u16;
    /// When `EXCLUSIVE` is true, saves with `O_EXCL`, meaning you
    /// have to unlink (or separate-file-and-rename) yourself! Note
    /// that if you give false, you should also use writable targets,
    /// i.e. use PERMS like 0o0644.
    const EXCLUSIVE: bool;

    fn from_reader<R: Read>(mut input: R) -> Result<Self> {
        let header: Self::Header = serde_json_read1(&mut input)?;
        header.check_version_and_kind(&Self::VERSION_AND_KIND)?;
        // Could use `serde_json::from_reader` instead to let it error
        // out; but maybe it's a good idea to allow json files to have
        // more stuff afterwards?
        Ok(serde_json_read1(&mut input)?)
    }

    fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let inp = BufReader::new(File::open(&path)?);
        Ok(Self::from_reader(inp)?)
    }

    fn write<W: Write>(&self, out: &mut W) -> Result<()> {
        let header = Self::Header::new_with_version_and_kind(&Self::VERSION_AND_KIND);
        serde_json::to_writer(&mut *out, &header)?;
        out.write_all(b"\n")?;
        serde_json::to_writer(&mut *out, self)?;
        out.write_all(b"\n")?;
        Ok(())
    }

    fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let overwrite_flag = if Self::EXCLUSIVE {
            OFlag::O_EXCL
        } else {
            OFlag::O_TRUNC
        };
        let flags = OFlag::O_CREAT | OFlag::O_WRONLY | overwrite_flag;
        let mode: Mode =
            Mode::from_bits(Self::PERMS.into()).expect("statically defined valid permission bits");
        let out = posix_open(&path, flags, mode)?;
        let mut out = BufWriter::new(out);
        self.write(&mut out)?;
        out.flush()?;
        Ok(())
    }
}
