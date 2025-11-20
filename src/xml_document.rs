//! Load an XML document and parse it into a read-only tree
//! representation efficiently. See
//! [`xml_document_map`](xml_document_map.rs) for carrying out simple
//! modifications.

use std::{fmt::Display, ops::Range, path::Path};

use anyhow::{Context, Result};
use ouroboros::self_referencing;
use pluraless::pluralized;
use roxmltree::{Document, Node, ParsingOptions};

/// Find elements with the given tag name without being in a namespace
/// (XX: danger?), append them to `output`. Do not recurse into found
/// nodes. `limit` is the maximum number of nodes found before it
/// stops and returns (it can push one more if called on an element
/// that matches).
pub fn find_elements_named<'a>(
    node: Node<'a, 'a>,
    element_name: &str,
    limit: usize,
    output: &mut Vec<Node<'a, 'a>>,
) {
    if node.tag_name().name() == element_name && node.tag_name().namespace().is_none() {
        output.push(node);
    } else {
        for child in node.children() {
            if output.len() >= limit {
                return;
            }
            find_elements_named(child, element_name, limit, output);
        }
    }
}

#[derive(Clone)]
pub struct XMLDocumentLocation<'a> {
    xmldocument: &'a XMLDocument,
    byte_range: Range<usize>,
}

impl<'a> XMLDocumentLocation<'a> {
    pub fn start_line_and_col(&self) -> (usize, usize) {
        str_line_col((0, 0), &self.xmldocument.as_str()[0..self.byte_range.start])
    }

    pub fn start_col(&self) -> usize {
        str_col(0, &self.xmldocument.as_str()[0..self.byte_range.start])
    }
}

/// Returns (line, column), based on `start`, of the end of `s` with
/// respect of the start of `s`, 0-based (for columns--for lines it
/// depends what you feed in). Note that column in `start` and in the
/// result is in characters, not bytes.
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

/// Returns the column, based on `start_col`, of the end of `s` with
/// respect of the start of `s`. Scans backwards from the end of `s`
/// to find the last newline before the end (if any), since this will
/// be faster in larger documents than scanning from the start (which
/// is what str_line_col does, necessarily since it has to count the
/// lines). Note that column in `start_col` and in the result is in
/// characters, not bytes.
fn str_col(start_col: usize, s: &str) -> usize {
    for (i, c) in s.chars().rev().enumerate() {
        match c {
            '\n' | '\r' => {
                return i;
            }
            _ => (),
        }
    }
    start_col
}

#[test]
fn t_str_col() {
    assert_eq!(str_col(0, ""), 0);
    assert_eq!(str_col(0, "hello"), 0);
    let t = |s: &str| str_col(s.len(), s);
    assert_eq!(t("hello"), 5);
    assert_eq!(t("hello\n"), 0);
    assert_eq!(t("hello\nworld!"), 6);
    assert_eq!(
        t("hello\nworld!\nmotör"),
        5 /* positions are in characters, not bytes! */
    );
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

        pluralized! { (end.0 - start.0) + 1  => lines }
        f.write_fmt(format_args!(
            "{lines}:columns {} – {}",
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

    /// Convert a position index into a `XMLDocumentLocation`.
    pub fn index_to_location(&self, index: usize) -> XMLDocumentLocation<'_> {
        XMLDocumentLocation {
            xmldocument: self,
            byte_range: index..index,
        }
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

    /// Find elements with the given tag name. `limit` is the maximum
    /// number of nodes found before it stops and returns (it can push
    /// one more if called on an element that matches).
    pub fn elements_named(&self, element_name: &str, limit: usize) -> Vec<Node<'_, '_>> {
        let mut output = Vec::new();
        find_elements_named(
            self.document().root_element(),
            element_name,
            limit,
            &mut output,
        );
        output
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
