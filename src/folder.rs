use std::collections::BTreeMap;

use anyhow::{bail, Result};
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};

use crate::{
    section::{Highlight, Section},
    xmlhub_fileinfo::FileInfo,
    xmlhub_indexer_defaults::HTML_ALLOCATOR_POOL,
};

/// An abstraction for folders and files, to collect the paths
/// reported by Git into, and then to map to nested
/// `Section`s. Contained files/folders are stored sorted by the name
/// of the file/folder inside this Folder; files and folders are
/// stored in separate struct fields, because their formatting will
/// also end up separately in a `Section` (files go to the intro,
/// folders to the subsections).
pub struct Folder<'f> {
    files: BTreeMap<String, &'f FileInfo>,
    folders: BTreeMap<String, Folder<'f>>,
}

impl<'f> Folder<'f> {
    pub fn new() -> Self {
        Self {
            files: BTreeMap::new(),
            folders: BTreeMap::new(),
        }
    }

    // This is just a helper method that recursively calls itself; see
    // `add` for the relevant wrapper method.
    fn add_(&mut self, segments: &[&str], file: &'f FileInfo) -> Result<()> {
        match segments {
            [] => unreachable!(),
            [segment, rest @ ..] => {
                if rest.is_empty() {
                    // `segment` being the last of the segments means
                    // that it represents the file name of the XML
                    // file itself
                    if let Some(oldfile) = self.files.get(*segment) {
                        bail!("duplicate file: {file:?} already entered as {oldfile:?}")
                    } else {
                        self.files.insert(segment.to_string(), file);
                    }
                } else {
                    if let Some(folder) = self.folders.get_mut(*segment) {
                        folder.add_(rest, file)?;
                    } else {
                        let mut folder = Folder::new();
                        folder.add_(rest, file)?;
                        self.folders.insert(segment.to_string(), folder);
                    }
                }
            }
        }
        Ok(())
    }

    /// Add a `FileInfo` to the right place in the `Folder` hierarchy
    /// according to the `FileInfo`'s `rel_path`, which is split into
    /// path segments.
    pub fn add(&mut self, file: &'f FileInfo) -> Result<()> {
        let segments: Vec<&str> = file.path.rel_path().split('/').collect();
        self.add_(&segments, file)
    }

    /// Convert to nested `Section`s.
    pub fn to_section(&self, title: Option<String>) -> Result<Section> {
        let intro = {
            let html = HTML_ALLOCATOR_POOL.get();

            // Create and then fill in a vector of boxes which we'll use
            // as the body for a `div` HTML element; this vector is a
            // custom vector implementation that allocates its storage
            // space from the `HtmlAllocator` in `html`, that's why we
            // allocate it via the `new_vec` method and not via
            // `Vec::new()`.
            let mut file_info_boxes = html.new_vec();
            for (file_name, file_info) in &self.files {
                file_info_boxes.push(file_info.to_info_box_html(&html, "box", file_name)?)?;
            }
            Some(html.preserialize(html.div([], file_info_boxes)?)?)
        };

        let subsections = self
            .folders
            .par_iter()
            .map(|(folder_name, folder)| {
                // Append a '/' to folder_name to indicate that those are
                // folder names
                folder.to_section(Some(format!("{folder_name}/")))
            })
            .collect::<Result<_>>()?;

        Ok(Section {
            highlight: Highlight::None,
            title,
            intro,
            subsections,
        })
    }
}
