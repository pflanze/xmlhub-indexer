use std::path::Path;

use anyhow::{Context, Result};
use roxmltree::{Document, ParsingOptions};

/// Representation of file contents that can be parsed from.
pub struct XMLDocumentBacking {
    string: String,
}

impl XMLDocumentBacking {
    /// Read the file from the given path.
    pub fn from_path(path: &Path) -> Result<Self> {
        // Back to reading the whole file to memory first since roxmltree
        // requires that.
        Ok(Self {
            string: std::fs::read_to_string(path).context("reading file")?,
        })
    }

    /// Parse the given file, return the comments *above the top
    /// element* as owned strings, and the parsed XML tree (which
    /// references the `XMLDocumentBacking`).
    pub fn parse(&self) -> Result<(Vec<String>, Document)> {
        let opt = ParsingOptions {
            allow_dtd: true,
            ..ParsingOptions::default()
        };
        let xmldoc =
            Document::parse_with_options(&self.string, opt).context("parsing the XML markup")?;

        let root = xmldoc.root();

        let comments: Vec<String> = root
            .children()
            .take_while(|item| item.is_comment())
            .map(|item| item.text().expect("comment has text").to_owned())
            .collect();

        Ok((comments, xmldoc))
    }
}
