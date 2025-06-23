use std::{ffi::OsStr, path::PathBuf, sync::Arc};

#[derive(Debug, Clone)]
pub struct BaseAndRelPath {
    pub base_path: Option<Arc<PathBuf>>,
    pub rel_path: PathBuf,
}

impl BaseAndRelPath {
    pub fn new(base_path: Option<Arc<PathBuf>>, rel_path: PathBuf) -> Self {
        Self {
            base_path,
            rel_path,
        }
    }

    pub fn full_path(&self) -> PathBuf {
        if let Some(path) = &self.base_path {
            let mut path = (**path).to_owned();
            path.push(&self.rel_path);
            path
        } else {
            self.rel_path.clone()
        }
    }

    pub fn extension(&self) -> Option<&OsStr> {
        self.rel_path.extension()
    }

    pub fn rel_path(&self) -> &str {
        self.rel_path
            // XXX what happens on Windows? UTF-16 always needs recoding, no?
            .to_str()
            .expect("always works since created from str")
    }
}
