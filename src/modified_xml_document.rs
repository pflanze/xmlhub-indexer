//! Modify XML documents. Only simple modifications supported
//! currently. Works on `XMLDocument`, and hence relies on the fast
//! `roxmltree` library, which is only a read-only
//! representation. Instead of mutating the tree directly place, work
//! with a set of deletions and inserts on the stringified
//! representation. Currently limited to inserting XML comments.

use std::{
    ops::Range,
    sync::atomic::{AtomicU64, Ordering},
};

use anyhow::Result;
use roxmltree::Node;

use crate::{
    modified_document::{Modification, ModifiedDocument},
    xml_document::XMLDocument,
};

/// Find elements with the given tag name without being in a namespace
/// (XXX: danger?), append them to `output`. Do not recurse into found
/// nodes.
pub fn find_elements_named<'a>(
    node: Node<'a, 'a>,
    element_name: &str,
    output: &mut Vec<Node<'a, 'a>>,
) {
    if node.tag_name().name() == element_name && node.tag_name().namespace().is_none() {
        output.push(node);
    } else {
        for child in node.children() {
            find_elements_named(child, element_name, output);
        }
    }
}

/// Turn a comment string to XML comment syntax, safely. Puts spaces
/// between subsequent '-' characters, is there any better approach?
/// If `s` contains '\n', puts the comment start and end on their own
/// lines and puts `indent` in front of all lines.
pub fn escape_comment(s: &str, indent: &str) -> String {
    let is_multiline = s.contains('\n');
    let mut out = String::from("<!--");
    if is_multiline {
        out.push_str("\n");
    } else {
        out.push(' ');
    }
    let mut last = None;
    for c in s.chars() {
        if let Some(last) = last {
            if last == '\n' {
                out.push_str(indent)
            }
            if c == '-' && last == '-' {
                out.push(' ');
            }
        }
        out.push(c);
        last = Some(c);
    }
    if is_multiline {
        out.push_str("\n");
    } else {
        out.push(' ');
    }
    out.push_str("-->");
    out
}

#[test]
fn t_escape_comment() {
    let t = |s| escape_comment(s, "  ");
    assert_eq!(t(""), "<!--  -->");
    assert_eq!(t("h"), "<!-- h -->");
    assert_eq!(t("-h-"), "<!-- -h- -->");
    assert_eq!(t("--"), "<!-- - - -->");
    assert_eq!(t("--a"), "<!-- - -a -->");
    assert_eq!(t("---"), "<!-- - - - -->");
}

// XX optim: Cow<str>
fn escape_text(s: &str) -> String {
    let append = |out: &mut Vec<u8>, bs: &[u8]| {
        // XX faster pls?
        for b in bs {
            out.push(*b);
        }
    };
    let mut out: Vec<u8> = Vec::new();
    for b in s.bytes() {
        match b {
            b'&' => append(&mut out, b"&amp;"),
            b'<' => append(&mut out, b"&lt;"),
            b'>' => append(&mut out, b"&gt;"),
            b'"' => append(&mut out, b"&quot;"),
            b'\'' => append(&mut out, b"&apos;"),
            _ => out.push(b),
        }
    }
    // XX use unsafe unchecked?
    String::from_utf8(out).expect("no bug")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DocumentId(u64);

fn new_document_id() -> DocumentId {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    DocumentId(COUNTER.fetch_add(1, Ordering::Relaxed))
}

pub struct ModifiedXMLDocument<'d> {
    id: DocumentId,
    xml_document: &'d XMLDocument,
    document: ModifiedDocument<'d>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentPosition {
    id: DocumentId,
    pub position: usize,
}

// #[derive(Debug, Clone, PartialEq, Eq)]
// pub struct DocumentRange {
//     id: DocumentId,
//     pub range: Range<usize>
// }

pub enum ClearAction<'t> {
    /// Clear the element body
    Element {
        /// If the element body is all whitespace, do not remove
        /// anything. Still remove whitespace together with other nodes if
        /// there are any.
        treat_whitespace_as_empty: bool,
    },
    /// Clear an attribute
    Attribute {
        /// Clear the attribute with the given name
        name: &'t str,
        /// Place this string instead -- XX CAREFUL: currently this
        /// must be the xml-escaped form of a string!
        replacement: &'t str,
    },
}

pub struct ClearElementsOpts<'t, 'a> {
    /// Actions to carry out on a found element
    pub actions: &'a [ClearAction<'t>],
    /// Prefix found nodes with the given comment string and indent if
    /// given.
    pub comment_and_indent: Option<(&'t str, &'t str)>,
    /// If is true, the comment is added even if there were no child
    /// nodes.
    pub always_add_comment: bool,
}

impl<'d> ModifiedXMLDocument<'d> {
    pub fn new(xml_document: &'d XMLDocument) -> Self {
        Self {
            id: new_document_id(),
            xml_document,
            document: ModifiedDocument::new(xml_document.as_str()),
        }
    }

    /// Panics if the given `position` is not for this document.
    pub fn assert_position(&self, position: DocumentPosition) -> usize {
        assert_eq!(self.id, position.id);
        position.position
    }

    /// The position above any existing comments or the root element
    /// (i.e. right after the XML declaration, usually). Returns None
    /// if the document has no cmment, element or text nodes.
    pub fn the_top(&self) -> Option<DocumentPosition> {
        let mut first_start = None;
        let root = self.xml_document.document().root();
        for item in root.children() {
            if item.is_comment() || item.is_element() || item.is_text() {
                first_start = Some(item.range().start);
                break;
            }
        }
        first_start.map(|position| DocumentPosition {
            id: self.id,
            position,
        })
    }

    /// Insert the given text as an XML comment at the given
    /// position. Panics if the given `DocumentPosition` is not for
    /// this document. NOTE: see docs on `escape_comment`.
    pub fn insert_comment_at(&mut self, position: DocumentPosition, comment: &str, indent: &str) {
        let mut escaped_comment = escape_comment(comment, indent);
        escaped_comment.push_str("\n");
        self.document.push(Modification::Insert(
            self.assert_position(position),
            escaped_comment.into(),
        ));
    }

    /// Insert the given text at the given position. It is properly
    /// escaped. Panics if the given `DocumentPosition` is not for
    /// this document. NOTE: inserting text other than whitespace is
    /// only valid in the child node area, outside an element tag
    /// (e.g. not at the position returned by `the_top`).
    pub fn insert_text_at(&mut self, position: DocumentPosition, text: &str) {
        self.document.push(Modification::Insert(
            self.assert_position(position),
            escape_text(text).into(),
        ));
    }

    /// Find elements with the given tag name
    pub fn elements_named(&self, element_name: &str) -> Vec<Node<'d, 'd>> {
        let mut output = Vec::new();
        find_elements_named(
            self.xml_document.document().root_element(),
            element_name,
            &mut output,
        );
        output
    }

    /// Find elements with the given tag name, regardless of their
    /// position in the document, delete them, and replace them with
    /// the given comment string and indent if given.
    pub fn delete_elements_named(
        &mut self,
        element_name: &str,
        comment_and_indent: Option<(&str, &str)>,
    ) {
        for element in self.elements_named(element_name) {
            let range = element.range();
            self.document.push(Modification::Delete(range.clone()));

            if let Some((comment, indent)) = &comment_and_indent {
                let escaped_comment = escape_comment(comment, indent);
                self.document
                    .push(Modification::Insert(range.start, escaped_comment.into()));
            }
        }
    }

    /// Find elements with the given tag name, regardless of their
    /// position in the document. See `ClearElementsOpts` for
    /// details. NOTE: leaves attributes in place! Returns how many
    /// elements were cleared.
    pub fn clear_elements_named<'actions>(
        &mut self,
        element_name: &str,
        opts: &ClearElementsOpts<'d, 'actions>,
    ) -> usize {
        let mut n_cleared = 0;
        for element in self.elements_named(element_name) {
            for action in opts.actions {
                match action {
                    ClearAction::Element {
                        treat_whitespace_as_empty,
                    } => {
                        let is_modified;
                        {
                            let mut delete_range: Option<Range<usize>> = None;
                            for node in element.children() {
                                if let Some(range) = delete_range.as_mut() {
                                    range.end = node.range().end;
                                } else {
                                    delete_range = Some(node.range());
                                }
                            }
                            if let Some(range) = delete_range {
                                if *treat_whitespace_as_empty
                                    && self.document.original_str()[range.clone()]
                                        .chars()
                                        .all(|c| c.is_ascii_whitespace())
                                {
                                    is_modified = false;
                                } else {
                                    self.document.push(Modification::Delete(range));
                                    is_modified = true;
                                }
                            } else {
                                is_modified = false;
                            }
                        }
                        if is_modified {
                            n_cleared += 1;
                        }
                    }
                    ClearAction::Attribute { name, replacement } => {
                        let is_modified;
                        if let Some(attribute) = element.attribute_node(*name) {
                            let range = attribute.range_value();
                            if &self.document.original_str()[range.clone()] == *replacement {
                                is_modified = false;
                            } else {
                                let start = range.start;
                                self.document.push(Modification::Delete(range));
                                if !replacement.is_empty() {
                                    self.document
                                        .push(Modification::Insert(start, (*replacement).into()));
                                }
                                is_modified = true;
                            }
                        } else {
                            is_modified = false;
                        }
                        if is_modified {
                            n_cleared += 1;
                        }
                    }
                }
            }

            if opts.always_add_comment || n_cleared > 0 {
                if let Some((comment, indent)) = &opts.comment_and_indent {
                    let escaped_comment = escape_comment(comment, indent);

                    // How to know how much to indent on the next line?
                    // Check the horizontal position of the position where
                    // we insert the comment:
                    let element_start_col = self
                        .xml_document
                        .index_to_location(element.range().start)
                        .start_col();
                    let indent = " ".repeat(element_start_col);

                    self.document.push(Modification::Insert(
                        element.range().start,
                        format!("{escaped_comment}\n{indent}").into(),
                    ));
                }
            }
        }
        n_cleared
    }

    pub fn to_string_and_modified(&mut self) -> Result<(String, bool)> {
        self.document.to_string_and_modified()
    }

    pub fn original_len(&self) -> usize {
        self.document.original_len()
    }

    pub fn len(&mut self) -> Result<usize> {
        self.document.len()
    }
}
