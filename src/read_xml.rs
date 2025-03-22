use std::{fmt::Display, ops::Range, path::Path};

use anyhow::{Context, Result};
use ouroboros::self_referencing;
use roxmltree::{Document, Node, ParsingOptions};

use crate::util::english_plural;

#[derive(Clone)]
pub struct XMLDocumentLocation<'a> {
    xmldocument: &'a XMLDocument,
    byte_range: Range<usize>,
}

/// Returns (line, column), based on `start`, of the end of `s` with
/// respect of the start of `s`.
fn str_line_col(start: (usize, usize), s: &str) -> (usize, usize) {
    let (mut line, mut col) = start;
    for c in s.chars() {
        match c {
            '\n' => {
                line += 1;
                col = 0;
            }
            '\r' => {
                col = 0;
            }
            _ => {
                col += 1;
            }
        }
    }
    (line, col)
}

/// Format line, col in the format as used by roxmltree itself, and
/// matching VS Code's numbering (but not Emacs' which is 1:0 based),
/// meaning as line:col and with line anc col both 1-based
fn line_col_string((line, col): (usize, usize)) -> String {
    format!("{}:{}", line + 1, col + 1)
}

impl<'a> Display for XMLDocumentLocation<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = self.xmldocument.as_str();
        let start = str_line_col((0, 0), &s[0..self.byte_range.start]);
        let end = str_line_col(start, &s[self.byte_range.start..self.byte_range.end]);
        let line_or_lines = english_plural((end.0 - start.0) + 1, "lines");
        f.write_fmt(format_args!(
            "{line_or_lines} {} – {}",
            line_col_string(start),
            line_col_string(end)
        ))
    }
}

pub struct XMLDocumentComment<'a> {
    pub location: XMLDocumentLocation<'a>,
    pub string: &'a str,
}

/// A parsed XML document: bundles the XML string and parsed
/// `roxmltree::Document`.
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

    /// The comments above the first element in the document.
    pub fn header_comments<'a>(&'a self) -> impl Iterator<Item = XMLDocumentComment<'a>> {
        let root: Node = self.document().root();
        root.children()
            .take_while(|item| item.is_comment())
            .map(|item| XMLDocumentComment {
                location: XMLDocumentLocation {
                    xmldocument: self,
                    byte_range: item.range(),
                },
                string: item.text().expect("comment has text"),
            })
    }
}

/// Load the given file into memory and parse it into a tree of
/// elements representation.
pub fn read_xml_file(path: &Path) -> Result<XMLDocument> {
    // Back to reading the whole file to memory first since roxmltree
    // requires that.
    let string = std::fs::read_to_string(path)
        .context("opening or reading the file contents")?
        .into_boxed_str();

    XMLDocument::try_new(string, |string| {
        let opt = ParsingOptions {
            allow_dtd: true,
            // nodes_limit: 1, -- somehow ignored
            ..ParsingOptions::default()
        };
        Document::parse_with_options(string, opt).context("parsing the XML markup")
    })
}
