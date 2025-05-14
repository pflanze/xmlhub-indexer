use std::{
    borrow::Cow,
    path::{Path, PathBuf},
};

use lazy_static::lazy_static;
use nix::NixPath;

lazy_static! {
    pub static ref CURRENT_DIRECTORY: &'static Path = ".".as_ref();
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
            (*CURRENT_DIRECTORY).into()
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
            (*CURRENT_DIRECTORY).into()
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
            (*CURRENT_DIRECTORY).into()
        } else {
            self.into()
        }
    }
}

#[test]
fn t_fixup() {
    assert_eq!(CURRENT_DIRECTORY.to_string_lossy(), ".");
    assert_eq!(&PathBuf::from(".").fixup(), *CURRENT_DIRECTORY);
    assert_eq!(&PathBuf::from("").fixup(), *CURRENT_DIRECTORY);
    assert_eq!(
        PathBuf::from("foo").fixup().as_ref(),
        AsRef::<Path>::as_ref("foo")
    );
    // BTW:
    assert_eq!(
        PathBuf::from("foo").fixup().as_ref(),
        AsRef::<Path>::as_ref("foo/")
    );
}
