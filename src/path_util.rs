//! Make it easy to append a segment to an existing path.

use std::path::{Path, PathBuf};

pub trait AppendToPath {
    /// Note: `segment` should be a single file/folder name and *not*
    /// contain `/` or `\\` characters!
    fn append(self, segment: &str) -> PathBuf;
}

impl<'p> AppendToPath for &'p Path {
    fn append(self, segment: &str) -> PathBuf {
        let mut path = self.to_owned();
        path.push(segment);
        path
    }
}

impl<'p> AppendToPath for &'p PathBuf {
    fn append(self, segment: &str) -> PathBuf {
        let mut path = self.clone();
        path.push(segment);
        path
    }
}

impl AppendToPath for PathBuf {
    fn append(mut self, segment: &str) -> PathBuf {
        self.push(segment);
        self
    }
}
