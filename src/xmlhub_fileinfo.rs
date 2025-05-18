//! Data structures to hold an attribute value, and the whole set of
//! values for a file after their extraction from it, as well as
//! operations (`impl` blocks) including parsing that information from
//! strings and formatting the information as HTML.

use std::{borrow::Cow, collections::BTreeMap, marker::PhantomData};

use ahtml::{att, flat::Flat, util::SoftPre, AId, ASlice, Element, HtmlAllocator, Node};
use anyhow::{bail, Result};
use pluraless::pluralized;

use crate::{
    git::BaseAndRelPath,
    util::{self, list_get_by_key},
    xmlhub_attributes::{
        sort_in_definition_order, AttributeKind, AttributeName, AttributeNeed, AttributeSource,
        AttributeSpecification, DerivationSpecification, METADATA_SPECIFICATION,
    },
    xmlhub_autolink::Autolink,
    xmlhub_file_issues::{FileIssues, FileWarnings},
    xmlhub_indexer_defaults::{
        document_symbol, BACK_TO_INDEX_SYMBOL, FILEINFO_METADATA_BGCOLOR, FILEINFO_PATH_BGCOLOR,
        FILEINFO_WARNINGS_BGCOLOR,
    },
};

/// A concrete attribute value: either a string, a list of strings, or
/// not present. It links the `AttributeSpecification` so that it can
/// be properly formatted and generate links back to the correct
/// index.
#[derive(Debug)]
pub struct AttributeValue {
    spec: &'static AttributeSpecification,
    value: AttributeValueKind,
}

#[derive(Debug)]
pub enum AttributeValueKind {
    String(String),
    StringList(Vec<String>),
    NA,
}

impl AttributeValue {
    /// Parse an input into the representation required by the given
    /// AttributeSpecification (like, a single string or
    /// lists). Returns an error if it couldn't do that, which happens
    /// if the input is only whitespace but a value is required by the
    /// spec.
    pub fn from_str_and_spec(val: &str, spec: &'static AttributeSpecification) -> Result<Self> {
        let source_spec = match &spec.source {
            AttributeSource::Specified(source_spec) => source_spec,
            AttributeSource::Derived(_) => bail!(
                "the value of the attribute {:?} is derived automatically, \
                 it cannot be specified manually; please remove the entry",
                spec.key.as_ref()
            ),
        };
        let value: AttributeValueKind = if val.is_empty() || val == "NA" {
            match source_spec.need {
                AttributeNeed::Optional => AttributeValueKind::NA,
                AttributeNeed::Required => {
                    bail!(
                        "attribute {:?} requires {}, but none given",
                        spec.key.as_ref(),
                        if source_spec.kind.is_list() {
                            "values"
                        } else {
                            "a value"
                        }
                    )
                }
            }
        } else {
            match source_spec.kind {
                AttributeKind::String {
                    normalize_whitespace,
                } => {
                    let value = val.trim();
                    let value = if normalize_whitespace {
                        util::normalize_whitespace(value)
                    } else {
                        value.into()
                    };
                    AttributeValueKind::String(value)
                }
                AttributeKind::StringList { input_separator } => {
                    // (Note: there is no need to replace '\n' with ' '
                    // in `val` first, because the trim will remove
                    // those around values, and normalize_whitespace will
                    // replace those within keys, too.)
                    let vals: Vec<String> = val
                        .split(input_separator)
                        .map(|s| util::normalize_whitespace(s.trim()))
                        .filter(|s| !s.is_empty())
                        .collect();
                    if vals.is_empty() {
                        match source_spec.need {
                            AttributeNeed::Optional => AttributeValueKind::NA,
                            AttributeNeed::Required => {
                                bail!(
                                    "values for attribute {:?} are required but missing",
                                    spec.key
                                )
                            }
                        }
                    } else {
                        AttributeValueKind::StringList(vals)
                    }
                }
            }
        };
        Ok(AttributeValue { spec, value })
    }

    /// Also works for single-value and unavailable attributes,
    /// returning a list of one or no entries, respectively. (`Cow`
    /// allows both sharing of existing vectors as well as holding new
    /// ones; that's just a performance feature, they can be used
    /// wherever a Vec or [] is required.)
    pub fn as_string_list(&self) -> Cow<[String]> {
        match &self.value {
            AttributeValueKind::StringList(value) => Cow::from(value.as_slice()),
            AttributeValueKind::NA => Cow::from(&[]),
            AttributeValueKind::String(value) => Cow::from(vec![value.clone()]),
        }
    }

    /// Convert the value, or whole value list in the case of
    /// StringList, to HTML. This is used for the file info boxes for
    /// both .html and .md files. An `ASlice<Node>` is a list of
    /// elements (nodes), directly usable as the body (child elements)
    /// for another element.
    fn to_html(&self, html: &HtmlAllocator) -> Result<Flat<Node>> {
        let AttributeValue { spec, value } = self;
        // Make a function `possibly_link_back` that takes the raw
        // `key_value` string and the prepared value and adds a link
        // to the index for `spec`key`, to the entry for `key_value`,
        // if the spec says it is indexed.
        let possibly_link_back = {
            let key_string_preparation = spec.indexing.key_string_preparation();
            move |key_value, body: Flat<Node>| -> Result<Flat<Node>> {
                if let Some(key_string_preparation) = &key_string_preparation {
                    let anchor_name = spec
                        .key
                        .anchor_name(&key_string_preparation.prepare_key_string(key_value));
                    let mut vec = html.new_vec();
                    vec.push_flat(body)?;
                    // vec.push(html.nbsp()?)?;
                    vec.push(html.a(
                        [
                            att("href", format!("#{anchor_name}")),
                            att("title", "jump to index entry"),
                        ],
                        html.text(BACK_TO_INDEX_SYMBOL)?,
                    )?)?;
                    Ok(Flat::Slice(vec.as_slice()))
                } else {
                    Ok(body)
                }
            }
        };
        match value {
            AttributeValueKind::String(value) => {
                let softpre = SoftPre {
                    tabs_to_nbsp: Some(8),
                    autolink: match spec.autolink {
                        Autolink::None => false,
                        Autolink::Web => true,
                        // XX never need to link DOI values in full
                        // text, do we? Currently silently ignored!
                        Autolink::Doi => false,
                    },
                    input_line_separator: "\n",
                };
                let body = softpre.format(value.trim(), html)?;
                let body_node: &Node = html.get_node(body).expect("just allocated");
                let body_element: &Element =
                    body_node.as_element().expect("softpre returns an element");
                // XX softpre must allow to omit the ending <br>! Hack:
                let full_body: ASlice<Node> = body_element.body;
                let (keep, _br) = full_body
                    .split_at(full_body.len() - 1)
                    .expect("always getting at least 1 br");
                let linked_slice = possibly_link_back(value, Flat::Slice(keep))?;
                html.element(body_element.meta, body_element.attr, linked_slice)
                    .map(Flat::One)
            }
            AttributeValueKind::StringList(value) => {
                let mut body = html.new_vec();
                let mut need_comma = false;
                for text in value {
                    if need_comma {
                        body.push(html.text(", ")?)?;
                    }
                    need_comma = true;
                    // Do not do SoftPre for string list items, but only
                    // autolink (if requested). Then wrap in <q></q>.
                    let text_marked_up = html.q([], spec.autolink.format_html(text, html)?)?;
                    body.push_flat(possibly_link_back(text, Flat::One(text_marked_up))?)?;
                }
                Ok(Flat::Slice(body.as_slice()))
            }
            AttributeValueKind::NA => html.i([], html.text("n.A.")?).map(Flat::One),
        }
    }
}

pub trait HavingDerivedValues {}

#[derive(Debug)]
pub struct WithoutDerivedValues;
impl HavingDerivedValues for WithoutDerivedValues {}

#[derive(Debug)]
pub struct WithDerivedValues;
impl HavingDerivedValues for WithDerivedValues {}

/// The concrete metadata values for one particular file, specified
/// via XML comments in it. The keys are the same as (or a subset of)
/// those in `METADATA_SPECIFICATION`.
#[derive(Debug)]
pub struct Metadata<H: HavingDerivedValues> {
    kind: PhantomData<H>,
    values: BTreeMap<AttributeName, AttributeValue>,
}

impl<H: HavingDerivedValues> Metadata<H> {
    pub fn new(values: BTreeMap<AttributeName, AttributeValue>) -> Self {
        Self {
            kind: Default::default(),
            values,
        }
    }

    /// Retrieve the value for an attribute name.
    pub fn get(&self, key: AttributeName) -> Option<&AttributeValue> {
        self.values.get(&key).or_else(|| {
            // Double check that the `key` is actually a valid
            // AttributeName before reporting that no value is present
            // for that key.
            if list_get_by_key(METADATA_SPECIFICATION, |spec| &spec.key, &key).is_none() {
                panic!("invalid AttributeName value {key:?}")
            }
            None
        })
    }

    /// The entries in the same order as given in
    /// `METADATA_SPECIFICATION`, with gaps where a key wasn't given
    /// in the file.
    fn sorted_entries(&self) -> Vec<(AttributeName, Option<&AttributeValue>)> {
        sort_in_definition_order(self.values.iter().map(|(k, v)| (*k, v)))
    }

    /// An HTML table with all metadata.
    fn to_html(&self, html: &HtmlAllocator) -> Result<AId<Node>> {
        let mut table_body = html.new_vec();
        for (attribute_name, opt_attval) in self.sorted_entries() {
            let attval_html: Flat<Node> = if let Some(attval) = opt_attval {
                attval.to_html(html)?
            } else {
                // Entry is missing in the file; show that fact.
                // (Also report that top-level as a warning? That
                // would be a bit ugly to implement.)
                Flat::One(html.i(
                    [
                        att("style", "color: red;"),
                        att(
                            "title",
                            format!(
                                "The XML comment for {attribute_name:?} is completely \
                                 missing in this file, perhaps because of an oversight."
                            ),
                        ),
                    ],
                    html.text("entry missing")?,
                )?)
            };
            table_body.push(html.tr(
                [],
                [
                    html.td(
                        [
                            att("class", "metadata_key"),
                            // The above CSS is lost via Markdown, thus also try:
                            att("valign", "top"),
                            att("align", "right"),
                        ],
                        html.i([], [html.text(attribute_name.as_ref())?, html.text(":")?])?,
                    )?,
                    html.td([att("class", "metadata_value")], attval_html)?,
                ],
            )?)?;
        }
        html.table([att("class", "metadata"), att("border", 0)], table_body)
    }
}

impl Metadata<WithoutDerivedValues> {
    /// Generate derived attribute values (as listed in
    /// `METADATA_SPECIFICATION`)
    pub fn extend(self, warnings: &mut Vec<String>) -> Metadata<WithDerivedValues> {
        let mut values = self.values;

        let mut from = Vec::new();
        for spec in METADATA_SPECIFICATION {
            if let AttributeSpecification {
                key,
                source:
                    AttributeSource::Derived(DerivationSpecification {
                        derived_from,
                        derivation,
                    }),
                autolink: _,
                indexing: _,
            } = spec
            {
                from.clear();
                for from_key in *derived_from {
                    from.push(values.get(from_key));
                }
                let value = derivation(&from, warnings);
                values.insert(*key, AttributeValue { spec, value });
            }
        }

        Metadata {
            kind: Default::default(),
            values,
        }
    }
}

/// The whole, concrete, information on one particular file.
#[derive(Debug)]
pub struct FileInfo<H: HavingDerivedValues> {
    pub id: usize,
    pub path: BaseAndRelPath,
    pub metadata: Metadata<H>,
    pub warnings: Vec<String>,
}

// For FileInfo to go into a BTreeSet (`BTreeSet<&FileInfo>` further
// below), it needs to be orderable. Only `id` is relevant for that
// (and the other types have no Ord implementation), thus write
// implementations manually:
impl<H: HavingDerivedValues> Ord for FileInfo<H> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.id.cmp(&other.id)
    }
}
impl<H: HavingDerivedValues> PartialOrd for FileInfo<H> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<H: HavingDerivedValues> PartialEq for FileInfo<H> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl<H: HavingDerivedValues> Eq for FileInfo<H> {}

impl<H: HavingDerivedValues> FileInfo<H> {
    /// Give a temporary FileWarnings object with the same trait as
    /// FileErrors, for warnings display.
    pub fn opt_warnings(&self) -> Option<FileWarnings> {
        if self.warnings.is_empty() {
            None
        } else {
            Some(FileWarnings {
                id: self.id,
                path: &self.path,
                warnings: &self.warnings,
            })
        }
    }

    /// Show in a box with a table of the metadata
    pub fn to_info_box_html(
        &self,
        html: &HtmlAllocator,
        id_prefix: &str,
        file_path_or_name: &str,
    ) -> Result<AId<Node>> {
        let id_string = format!("{id_prefix}-{}", self.id);
        html.a(
            [att("name", &id_string)],
            html.table(
                [
                    att("id", &id_string),
                    att("class", "fileinfo"),
                    att("border", 0),
                ],
                [
                    html.tr(
                        [],
                        html.td(
                            [
                                att("class", "fileinfo_path"),
                                att("bgcolor", FILEINFO_PATH_BGCOLOR),
                            ],
                            html.b(
                                [],
                                html.a(
                                    [
                                        att(
                                            "href",
                                            // (This would need path
                                            // calculation if the index files
                                            // weren't written to the
                                            // top-level directory)
                                            self.path.rel_path(),
                                        ),
                                        att("title", "Open the file"),
                                    ],
                                    [
                                        html.text(file_path_or_name)?,
                                        html.nbsp()?,
                                        document_symbol(html)?,
                                    ],
                                )?,
                            )?,
                        )?,
                    )?,
                    html.tr(
                        [att("class", "fileinfo_metadata")],
                        html.td(
                            [att("bgcolor", FILEINFO_METADATA_BGCOLOR)],
                            self.metadata.to_html(html)?,
                        )?,
                    )?,
                    if let Some(warnings) = self.opt_warnings() {
                        pluralized! { warnings.issues().len() => Warnings }
                        html.tr(
                            [att("class", "fileinfo_warnings")],
                            html.td(
                                [att("bgcolor", FILEINFO_WARNINGS_BGCOLOR)],
                                [
                                    html.div([], html.b([], html.text(format!("{Warnings}:"))?)?)?,
                                    html.div(
                                        [],
                                        warnings.to_html(
                                            false, // XX where is this defined?
                                            "box", html,
                                        )?,
                                    )?,
                                ],
                            )?,
                        )?
                    } else {
                        html.empty_node()?
                    },
                ],
            )?,
        )
    }
}
