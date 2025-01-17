use std::{fs::File, path::Path};

use anyhow::{Context, Result};
use memmap2::Mmap;
use ouroboros::self_referencing;
use roxmltree::{Document, ParsingOptions};

/// Representation of file contents that can be parsed from.
#[self_referencing]
pub struct XMLDocumentBacking {
    file: File,
    #[borrows(file)]
    map: Mmap,
    #[borrows(map)]
    string: &'this str,
}

impl XMLDocumentBacking {
    /// Read the file from the given path.
    pub fn from_path(path: &Path) -> Result<Self> {
        let file = File::open(path).context("opening file for reading")?;
        Ok(Self::try_new::<anyhow::Error>(
            file,
            |file: &File| Ok(unsafe { Mmap::map(file) }?),
            |map: &Mmap| Ok(std::str::from_utf8(&map[..])?),
        )?)
    }

    /// Parse the given file, return the comments *above the top
    /// element* as owned strings, and the parsed XML tree (which
    /// references the `XMLDocumentBacking`).
    pub fn parse(&self) -> Result<(Vec<String>, Document)> {
        let opt = ParsingOptions {
            allow_dtd: true,
            ..ParsingOptions::default()
        };
        let xmldoc = Document::parse_with_options(&*self.borrow_string(), opt)
            .context("parsing the XML markup")?;

        let root = xmldoc.root();

        let comments: Vec<String> = root
            .children()
            .take_while(|item| item.is_comment())
            .map(|item| item.text().expect("comment has text").to_owned())
            .collect();

        Ok((comments, xmldoc))
    }
}
