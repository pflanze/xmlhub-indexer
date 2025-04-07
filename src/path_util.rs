use std::{
    borrow::Cow,
    ffi::{OsStr, OsString},
    os::unix::prelude::OsStringExt,
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

// Add an extension to a path with a filename. Returns false if it
// does not in fact have a filename. `extension` must not include the
// dot. If `extension` is empty, nothing is appended (not even the
// dot). This function exists because the `add_extension` method in
// std is currently an unstable library feature.
pub fn add_extension<S: AsRef<OsStr>>(this: &mut PathBuf, extension: S) -> bool {
    _add_extension(this, extension.as_ref())
}

fn _add_extension(this: &mut PathBuf, extension: &OsStr) -> bool {
    let file_name = match this.file_name() {
        None => return false,
        Some(f) => f.as_encoded_bytes(),
    };

    let mut new = extension.as_encoded_bytes().to_vec();
    if !new.is_empty() {
        // "truncate until right after the file name
        // this is necessary for trimming the trailing slash"

        // Hmm, dunno. This is not going to behave the same, but I'm
        // happy with just appending a dot and the extension, please.

        let mut file_name: Vec<u8> = Vec::from(file_name);
        file_name.push(b'.');
        file_name.append(&mut new);

        // XX this depends on Unix, sigh.
        let file_name = OsString::from_vec(file_name);
        this.set_file_name(file_name);
    }

    true
}

#[test]
fn t_add_extension() {
    let t = |path: &str, ext: &str| {
        let mut path = PathBuf::from(path);
        if add_extension(&mut path, ext) {
            path.to_string_lossy().to_string()
        } else {
            format!("{path:?} -- unchanged")
        }
    };

    assert_eq!(t("hello", ""), "hello");
    assert_eq!(t("hello", "foo"), "hello.foo");
    assert_eq!(t("hello.foo", "bar"), "hello.foo.bar");
    assert_eq!(t("hello", ".foo"), "hello..foo");
    assert_eq!(t("/", ".foo"), "\"/\" -- unchanged");
    assert_eq!(t("hello/", ".foo"), "hello..foo"); // XX oh, buggy. todo fix
}
