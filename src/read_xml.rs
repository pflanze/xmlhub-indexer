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

    /// Parse the given file, return the comments *above the top element*,
    /// and if `parse_tree` is true also the whole tree of XML elements
    /// (which does not include comments). Even if not making use of the
    /// Element tree, it could be a good idea to generate it to detect
    /// when a file is not well-formed XML. (But currently actually always
    /// builds the tree.)
    pub fn parse(&self, build_tree: bool) -> Result<(Vec<String>, Option<Document>)> {
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

        let xmldoc = if build_tree { Some(xmldoc) } else { None };

        Ok((comments, xmldoc))
    }
}
