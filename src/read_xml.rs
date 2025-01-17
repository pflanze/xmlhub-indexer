use std::path::Path;

use anyhow::{anyhow, Context, Result};
use ouroboros::self_referencing;
use roxmltree::{Document, ParsingOptions};

/// Bundles the XML string and parsed `roxmltree::Document`.
#[self_referencing]
pub struct XMLDocument {
    string: Box<str>,
    #[borrows(string)]
    #[covariant]
    document: Document<'this>,
}

impl XMLDocument {
    pub fn as_str(&self) -> &str {
        self.borrow_string()
    }

    pub fn document<'a>(&'a self) -> &'a Document<'a> {
        self.borrow_document()
    }
}

/// Parse the given file, return the comments *above the top element*,
/// and the parsed XML tree.
pub fn read_xml_file(path: &Path) -> Result<(Vec<String>, XMLDocument)> {
    (|| -> Result<_> {
        // Back to reading the whole file to memory first since roxmltree
        // requires that.
        let string = std::fs::read_to_string(path)
            .context("reading file")?
            .into_boxed_str();

        let xmldoc = XMLDocument::try_new(string, |string| {
            let opt = ParsingOptions {
                allow_dtd: true,
                ..ParsingOptions::default()
            };
            Document::parse_with_options(string, opt).context("parsing the XML markup")
        })?;

        let root = xmldoc.document().root();

        let comments: Vec<String> = root
            .children()
            .take_while(|item| item.is_comment())
            .map(|item| item.text().expect("comment has text").to_owned())
            .collect();

        Ok((comments, xmldoc))
    })()
    .with_context(|| anyhow!("reading file {path:?}"))
}
