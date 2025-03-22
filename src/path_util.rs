use std::{
    borrow::Cow,
    path::{Path, PathBuf},
};

use nix::NixPath;

/// Make it easy to append a segment to an existing path.
pub trait AppendToPath {
    /// Note: `segment` should be a single file/folder name and *not*
    /// contain `/` or `\\` characters!
    fn append<P: AsRef<Path>>(self, segment: P) -> PathBuf;
}

impl<'p> AppendToPath for &'p Path {
    fn append<P: AsRef<Path>>(self, segment: P) -> PathBuf {
        let mut path = self.to_owned();
        path.push(segment);
        path
    }
}

impl<'p> AppendToPath for &'p PathBuf {
    fn append<P: AsRef<Path>>(self, segment: P) -> PathBuf {
        let mut path = self.clone();
        path.push(segment);
        path
    }
}

impl AppendToPath for PathBuf {
    fn append<P: AsRef<Path>>(mut self, segment: P) -> PathBuf {
        self.push(segment);
        self
    }
}

// XXX: how does this fare with Windows?
/// Replace the "" path with "."
pub trait FixupPath<'t> {
    fn fixup(self) -> Cow<'t, Path>
    where
        Self: 't;
}

impl<'t> FixupPath<'t> for &'t Path {
    fn fixup(self) -> Cow<'t, Path>
    where
        Self: 't,
    {
        if self.is_empty() {
            PathBuf::from(".").into()
        } else {
            self.into()
        }
    }
}

impl<'t> FixupPath<'t> for &'t PathBuf {
    fn fixup(self) -> Cow<'t, Path>
    where
        Self: 't,
    {
        if self.is_empty() {
            PathBuf::from(".").into()
        } else {
            self.into()
        }
    }
}

impl<'t> FixupPath<'t> for PathBuf {
    fn fixup(self) -> Cow<'t, Path>
    where
        Self: 't,
    {
        if self.is_empty() {
            PathBuf::from(".").into()
        } else {
            self.into()
        }
    }
}
