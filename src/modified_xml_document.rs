//! Modify XML documents. Only simple modifications supported
//! currently. Works on `XMLDocument`, and hence relies on the fast
//! `roxmltree` library, which is only a read-only
//! representation. Instead of mutating the tree directly place, work
//! with a set of deletions and inserts on the stringified
//! representation. Currently limited to inserting XML comments.

use std::ops::Range;

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

pub struct ModifiedXMLDocument<'d> {
    xml_document: &'d XMLDocument,
    document: ModifiedDocument<'d>,
}

impl<'d> ModifiedXMLDocument<'d> {
    pub fn new(xml_document: &'d XMLDocument) -> Self {
        Self {
            xml_document,
            document: ModifiedDocument::new(xml_document.as_str()),
        }
    }

    /// Insert the given text as an XML comment above any existing
    /// comments or the root element (i.e. right after the XML
    /// declaration, usually). NOTE: see docs on `escape_comment`.
    pub fn insert_comment_at_the_top(&mut self, comment: &str, indent: &str) {
        let mut first_start = None;
        let root = self.xml_document.document().root();
        for item in root.children() {
            if item.is_comment() || item.is_element() || item.is_text() {
                first_start = Some(item.range().start);
                break;
            }
        }
        let mut escaped_comment = escape_comment(comment, indent);
        escaped_comment.push_str("\n");
        self.document.push(Modification::Insert(
            first_start.unwrap_or(0),
            escaped_comment.into(),
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
    /// position in the document, delete their child nodes if any, and
    /// prefix them with the given comment string and indent if
    /// given. If `always_add_comment` is true, the comment is added
    /// even if there were no child nodes.  NOTE: leaves attributes in
    /// place! Returns how many elements were cleared.
    pub fn clear_elements_named(
        &mut self,
        element_name: &str,
        comment_and_indent: Option<(&str, &str)>,
        always_add_comment: bool,
    ) -> usize {
        let mut n_cleared = 0;
        for element in self.elements_named(element_name) {
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
                    self.document.push(Modification::Delete(range));
                    is_modified = true;
                } else {
                    is_modified = false;
                }
            };
            if is_modified {
                n_cleared += 1;
            }

            if always_add_comment || is_modified {
                if let Some((comment, indent)) = &comment_and_indent {
                    let escaped_comment = escape_comment(comment, indent);

                    // How to know how much to indent on the next line?
                    // Check the horizontal position of the position where
                    // we insert the comment:
                    let (_element_start_line, element_start_col) = self
                        .xml_document
                        .index_to_location(element.range().start)
                        .start_line_and_col();
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
}
