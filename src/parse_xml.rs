use std::path::Path;

use anyhow::{anyhow, Context, Result};
use xml::{reader::XmlEvent, EventReader, ParserConfig};
use xmltree::Element;

/// Parse the given file, return the comments *above the top element*,
/// and the whole tree of XML elements (which does not include
/// comments). Even if not making use of the Element tree, it's a good
/// idea for `parse_xml_file` to generate it, to detect when a file is
/// not well-formed XML.
pub fn parse_xml_file(path: &Path) -> Result<(Vec<String>, Element)> {
    let bytes = std::fs::read(path).with_context(|| anyhow!("reading file {path:?}"))?;

    // Parse `bytes` as item stream to extract the comments.
    let config = ParserConfig::new().ignore_comments(false);
    let input = EventReader::new_with_config(&*bytes, config);
    let mut comments = Vec::new();
    for item in input {
        let item = item.with_context(|| anyhow!("parsing file {path:?}"))?;
        match item {
            XmlEvent::Comment(comment) => comments.push(comment),
            XmlEvent::StartElement {
                name: _,
                attributes: _,
                namespace: _,
            } => break,
            // ignore all other items:
            _ => (),
        }
    }

    // Parse the bytes again, now building an element tree.
    let xmldoc = Element::parse(&*bytes).with_context(|| anyhow!("reparsing file {path:?}"))?;

    Ok((comments, xmldoc))
}
