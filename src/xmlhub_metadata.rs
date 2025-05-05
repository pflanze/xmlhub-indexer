//! Description of the valid metadata attributes and how they are
//! parsed and displayed. First, the definition of the data types for
//! the metadata (`struct`, `enum`, `impl`), then, using those, the
//! actual description in `METADATA_SPECIFICATION`.

use std::{collections::BTreeMap, fmt::Display};

use ahtml::{att, util::SoftPre, AId, HtmlAllocator, Node};
use ahtml_from_markdown::markdown::markdown_to_html;
use anyhow::Result;
use lazy_static::lazy_static;

use crate::{
    html_util::extract_paragraph_body,
    util::{self, format_anchor_name},
    xmlhub_autolink::Autolink,
};

/// An attribute name is a string that identifies an attribute. The
/// string is in the canonical casing as it should be shown in
/// metadata listings in the HTML/Markdown output. To try to avoid
/// making mistakes, we define a wrapper struct `AttributeName` to
/// make it clear everywhere whether we're having a string in
/// canonical casing or not.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct AttributeName(&'static str);

/// Get the actual attribute name string
impl AsRef<str> for AttributeName {
    fn as_ref(&self) -> &'static str {
        self.0
    }
}

impl AttributeName {
    /// Generate an anchor name for this attribute with the given
    /// attribute item string.
    pub fn anchor_name(self, key_string: &str) -> String {
        format!(
            "{}-{}",
            format_anchor_name(self.as_ref()),
            format_anchor_name(key_string)
        )
    }
}

/// Specifies whether an attribute is required
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum AttributeNeed {
    Optional,
    /// Not "NA", nor the empty string / space only, and for lists not
    /// the empty list (even a list of empty elements like ", , ," is
    /// not OK)
    Required,
}

/// Specifies how an attribute value should be treated
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum AttributeKind {
    /// A single piece of text, e.g. description or comment. It is
    /// formatted to HTML via `SoftPre`, meaning line breaks and tab
    /// characters are preserved.
    String {
        /// Whether to convert groups of any kind of whitespace
        /// (spaces, tabs, newlines) to a single space. I.e. this
        /// strips space based "markup" if true. Note that in indexes,
        /// values are normalized anyway, so this matters only for the
        /// display in the file info boxes. (StringList items (the
        /// case below) are always normalized btw.)
        normalize_whitespace: bool,
    },
    /// A list of small pieces of text, e.g. keywords. The individual
    /// list elements are cleaned up then formatted to HTML, all
    /// whitespace including line breaks is uniformly replaced with a
    /// single normal space.
    StringList {
        /// This is the separator as used between list items, in the
        /// XML files within the `<!-- -->` parts; e.g. if the items are
        /// separated by spaces, give " ", if separated by commas, give
        /// ",". This does not determine what's used for the HTML
        /// formatting; for that, see the `to_html` method on
        /// AttributeValue.
        input_separator: &'static str,
    },
}

fn text_not(is: bool) -> &'static str {
    if is {
        ""
    } else {
        "not "
    }
}

impl AttributeKind {
    pub fn is_list(&self) -> bool {
        match self {
            AttributeKind::String {
                normalize_whitespace: _,
            } => false,
            AttributeKind::StringList { input_separator: _ } => true,
        }
    }

    fn to_html(&self, html: &HtmlAllocator) -> Result<AId<Node>> {
        let softpre = SoftPre::default();
        match self {
            AttributeKind::String {
                normalize_whitespace,
            } => softpre.format(
                &format!(
                    "text with space {}normalized",
                    text_not(*normalize_whitespace),
                ),
                html,
            ),
            AttributeKind::StringList { input_separator } => softpre.format(
                &format!("list with items separated by {input_separator:?}",),
                html,
            ),
        }
    }
}

/// Whether an index should be created
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum AttributeIndexing {
    Index {
        /// Whether only the first word of each item should be used
        /// for indexing (useful for package names given with version
        /// number after it, to index the package name without the
        /// version).
        first_word_only: bool,
        /// Whether to convert the user-given values to lowercase for
        /// the index
        use_lowercase: bool,
    },
    NoIndex,
}

impl AttributeIndexing {
    pub fn key_string_preparation(&self) -> Option<KeyStringPreparation> {
        match *self {
            AttributeIndexing::Index {
                first_word_only,
                use_lowercase,
            } => Some(KeyStringPreparation {
                first_word_only,
                use_lowercase,
            }),
            AttributeIndexing::NoIndex => None,
        }
    }
    fn to_html(&self, is_list: bool, html: &HtmlAllocator) -> Result<AId<Node>> {
        let softpre = SoftPre::default();
        match self {
            AttributeIndexing::Index {
                first_word_only,
                use_lowercase,
            } => softpre.format(
                &format!(
                    "{}{}indexed,\n{}lower-cased",
                    if *first_word_only {
                        "first word "
                    } else {
                        "full value "
                    },
                    if is_list { "of each item " } else { "" },
                    text_not(*use_lowercase)
                ),
                html,
            ),
            AttributeIndexing::NoIndex => html.text(format!("not indexed")),
        }
    }
}

/// All metainformation on an attribute (its name, format, indexing
/// requirements..).
#[derive(Debug)]
pub struct AttributeSpecification {
    pub key: AttributeName,
    /// Description for the "Metainfo attributes" help file, in
    /// Markdown format
    desc: &'static str,
    pub need: AttributeNeed,
    pub kind: AttributeKind,
    pub autolink: Autolink,
    pub indexing: AttributeIndexing,
}

impl AttributeSpecification {
    const TITLES: &[&str] = &[
        "Name",
        "Description",
        "Content needed?",
        "Content kind",
        "URLs automatically linked?",
        "Indexing",
    ];

    /// Show the specification using HTML markup, for writing to
    /// ATTRIBUTE_SPECIFICATION_FILENAME.
    fn to_html(&self, html: &HtmlAllocator) -> Result<AId<Node>> {
        let AttributeSpecification {
            key,
            desc,
            need,
            kind,
            autolink,
            indexing,
        } = self;
        let desc_html = markdown_to_html(desc, html)?.html();
        // markdown_to_html wraps paragraphs in <p> even if it's just
        // one of them; strip that if possible:
        let desc_stripped = extract_paragraph_body(desc_html, true, html);

        html.tr(
            [],
            [
                html.td([], html.i([], html.text(key.as_ref())?)?)?,
                html.td([], desc_stripped)?,
                html.td(
                    [],
                    html.text(match need {
                        AttributeNeed::Optional => "optional",
                        AttributeNeed::Required => "required",
                    })?,
                )?,
                html.td([], kind.to_html(html)?)?,
                html.td([], html.text(autolink.to_text())?)?,
                html.td([], indexing.to_html(kind.is_list(), html)?)?,
            ],
        )
    }
}

/// Show the specification as plain text, for writing to the terminal
/// via the `help-attributes` subcommand
impl Display for AttributeSpecification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let AttributeSpecification {
            key,
            desc,
            need,
            kind,
            autolink,
            indexing,
        } = self;
        f.write_fmt(format_args!("  {}:\n", key.as_ref()))?;
        f.write_fmt(format_args!("      {desc}\n"))?;
        f.write_fmt(format_args!("    need: {need:?}\n"))?;
        f.write_fmt(format_args!("    kind: {kind:?}\n"))?;
        f.write_fmt(format_args!(
            "    autolink: {}\n",
            autolink.to_text() // XX too long?
        ))?;
        f.write_fmt(format_args!("    indexing: {indexing:?}\n"))?;
        Ok(())
    }
}

pub fn specifications_to_html(html: &HtmlAllocator) -> Result<AId<Node>> {
    let head: Vec<_> = AttributeSpecification::TITLES
        .iter()
        .map(|s| html.td([att("bgcolor", "#e0e0e0")], html.b([], html.text(s)?)?))
        .collect::<Result<_>>()?;
    let mut body = html.new_vec();
    for spec in METADATA_SPECIFICATION {
        body.push(spec.to_html(html)?)?;
    }
    html.table(
        [att("border", 1)],
        [html.thead([], html.tr([], head)?)?, html.tbody([], body)?],
    )
}

/// Description of the metadata attributes, what they must contain,
/// and how they are indexed. The order of entries here is also the
/// same order used for showing the extracted info in the info boxes
/// in the index pages.
pub const METADATA_SPECIFICATION: &[AttributeSpecification] = {
    &[
        AttributeSpecification {
            key: AttributeName("Keywords"),
            desc: "Words for the keyword index, for useful finding.",
            need: AttributeNeed::Required,
            kind: AttributeKind::StringList {
                input_separator: ",",
            },
            autolink: Autolink::Web,
            indexing: AttributeIndexing::Index {
                first_word_only: false,
                use_lowercase: true,
            },
        },
        AttributeSpecification {
            key: AttributeName("Version"),
            desc: "The BEAST version used, like \"2.7.1\".",
            need: AttributeNeed::Required,
            kind: AttributeKind::String {
                normalize_whitespace: false,
            },
            autolink: Autolink::Web,
            indexing: AttributeIndexing::Index {
                first_word_only: false,
                use_lowercase: false,
            },
        },
        AttributeSpecification {
            key: AttributeName("Packages"),
            desc: "The BEAST packages used (package name and version after a space).",
            need: AttributeNeed::Required,
            kind: AttributeKind::StringList {
                input_separator: ",",
            },
            autolink: Autolink::Web,
            indexing: AttributeIndexing::Index {
                first_word_only: true,
                use_lowercase: false,
            },
        },
        AttributeSpecification {
            key: AttributeName("Description"),
            desc: "A description of the work / contex, can be multiple lines.",
            need: AttributeNeed::Optional,
            kind: AttributeKind::String {
                normalize_whitespace: false,
            },
            autolink: Autolink::Web,
            indexing: AttributeIndexing::NoIndex,
        },
        AttributeSpecification {
            key: AttributeName("Comments"),
            desc: "Additional comments.", // XX what is the thinking behind it, really?
            need: AttributeNeed::Optional,
            kind: AttributeKind::String {
                normalize_whitespace: false,
            },
            autolink: Autolink::Web,
            indexing: AttributeIndexing::NoIndex,
        },
        AttributeSpecification {
            key: AttributeName("DOI"),
            desc: "DOI of papers that this file was used for, or that describe it.",
            need: AttributeNeed::Optional,
            kind: AttributeKind::StringList {
                input_separator: ",",
            },
            autolink: Autolink::Doi,
            indexing: AttributeIndexing::Index {
                first_word_only: false,
                use_lowercase: false,
            },
        },
        AttributeSpecification {
            key: AttributeName("Citation"),
            desc: "Papers for which no DOI could be provided under `DOI`. Do *not* \
                   provide information about papers here for which you have provided the `DOI`!",
            need: AttributeNeed::Optional,
            kind: AttributeKind::StringList {
                input_separator: "|",
            },
            autolink: Autolink::Web,
            indexing: AttributeIndexing::Index {
                first_word_only: false,
                use_lowercase: false,
            },
        },
        AttributeSpecification {
            key: AttributeName("Contact"),
            desc: "Whom to contact (and how) for more information on this file.",
            need: AttributeNeed::Required,
            kind: AttributeKind::String {
                normalize_whitespace: false,
            },
            autolink: Autolink::Web,
            indexing: AttributeIndexing::Index {
                first_word_only: false,
                use_lowercase: false,
            },
        },
        AttributeSpecification {
            key: AttributeName("Repository"),
            desc: "Original repository for the xml file.",
            need: AttributeNeed::Optional,
            kind: AttributeKind::String {
                normalize_whitespace: false,
            },
            autolink: Autolink::Web,
            indexing: AttributeIndexing::Index {
                first_word_only: false,
                use_lowercase: false,
            },
        },
    ]
};

// `lazy_static` sets things up so that the data for the given
// constant (`METADATA_KEY_POSITION`) is calculated when it is read
// for the first time.
lazy_static! {
    /// A mapping from an attribute name to its position; used for
    /// sorting the user-provided metadata entries uniformly.
    pub static ref METADATA_KEY_POSITION: BTreeMap<AttributeName, usize> = METADATA_SPECIFICATION
        .iter()
        .enumerate()
        .map(|(i, spec)| (spec.key, i))
        .collect();
}

/// Settings and a method for the conversion of a value (string) into
/// the key string to be used in an index entry (e.g. an item of
/// `Packages` like "BDSKY 1.2.3" might be converted to "BDSKY", or a
/// `Keywords` entry "Sampling-through-time" to
/// "sampling-through-time").
pub struct KeyStringPreparation {
    first_word_only: bool,
    use_lowercase: bool,
}

impl KeyStringPreparation {
    pub fn prepare_key_string(&self, key_string: &str) -> String {
        let normalized = util::normalize_whitespace(key_string.trim());
        // ^ Should we keep newlines instead, and then SoftPre for the
        // display? Probably not.
        let part = if self.first_word_only {
            normalized
                .split(' ')
                .next()
                .expect("key_string is not empty")
        } else {
            &normalized
        };
        if self.use_lowercase {
            part.to_lowercase()
        } else {
            part.into()
        }
    }
}
