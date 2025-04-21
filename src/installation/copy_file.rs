//! `copy` but provided as `Effect`, also removing the old target if
//! present and informing about it.

use std::{
    fmt::Debug,
    fs::{copy, remove_file},
    marker::PhantomData,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Context, Result};

use crate::effect::Effect;

use super::done::Done;

#[derive(Debug)]
pub struct CopyFile<R> {
    phantom: PhantomData<fn() -> R>,
    remove_existing_target: bool,
    source_path: PathBuf,
    target_path: PathBuf,
}

#[derive(Debug)]
pub struct CopiedFile<R> {
    pub provided: R,
    /// Todo: get rid of `Done`, just rely on the `show_bullet_points`
    /// view of `Effect`, right?
    pub done: Done,
}

impl<R: Debug> Effect for CopyFile<R> {
    type Requires = R;

    type Provides = CopiedFile<R>;

    fn show_bullet_points(&self) -> String {
        let Self {
            phantom: _,
            remove_existing_target,
            source_path,
            target_path,
        } = self;
        let replacing = if *remove_existing_target {
            ", replacing the latter"
        } else {
            ""
        };
        format!("  * copy the file from {source_path:?} to {target_path:?}{replacing}")
    }

    fn run(self: Box<Self>, provided: Self::Requires) -> Result<Self::Provides> {
        let Self {
            phantom: _,
            remove_existing_target,
            source_path,
            target_path,
        } = *self;
        let replacing = if remove_existing_target && target_path.exists() {
            remove_file(&target_path)
                .with_context(|| anyhow!("removing existing file {target_path:?}"))?;
            ", replacing the latter"
        } else {
            ""
        };

        copy(&source_path, &target_path)
            .with_context(|| anyhow!("copying file from {source_path:?} to {target_path:?}"))?;

        Ok(CopiedFile {
            provided,
            done: format!("copy file from {source_path:?} to {target_path:?}{replacing}").into(),
        })
    }
}

/// Return the action to copy a file.
pub fn copy_file<R: Debug>(source_path: &Path, target_path: &Path) -> CopyFile<R> {
    CopyFile {
        phantom: PhantomData,
        remove_existing_target: target_path.exists(),
        source_path: source_path.to_owned(),
        target_path: target_path.to_owned(),
    }
}
