// From the standard library
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{stderr, stdout, BufWriter, Write},
    path::PathBuf,
    process::exit,
};

// From external dependencies
use ahtml::{att, flat::Flat, util::SoftPre, AId, ASlice, HtmlAllocator, Node, Print, ToASlice};
use anyhow::{anyhow, bail, Context, Result};
use chrono::Local;
use clap::Parser;
use itertools::Itertools;
use lazy_static::lazy_static;

// From src/*.rs
use xmlhub_indexer::{
    browser::spawn_browser,
    flattened::Flattened,
    git::{git, git_ls_files, git_status, RelPathWithBase},
    git_check_version::GitLogVersionChecker,
    git_version::{GitVersion, SemVersion},
    parse_xml::parse_xml_file,
    util,
    util::{append, list_get_by_key, InsertValue},
};

const PROGRAM_NAME: &str = "xmlhub-indexer";
const PROGRAM_REPOSITORY: &str = "https://cevo-git.ethz.ch/cevo-resources/xmlhub-indexer";
const HTML_FILENAME: &str = "README.html";
const MD_FILENAME: &str = "README.md";
const CONTRIBUTE_FILE_NAME: &str = "CONTRIBUTE"; // without the .md or .html suffix!
const INFO_SYMBOL: &str = "ℹ️";

// =============================================================================
// Specification of the command line interface, using the `clap`
// library crate.

#[derive(clap::Parser, Debug)]
#[clap(next_line_help = true)]
/// Build an index of the files in the (non-public) XML Hub of the
/// cEvo group at the D-BSSE, ETH Zurich.
struct Opts {
    /// Show the program version. It was copied from `git describe
    /// --tags` at compile time.
    // Note: can't name this field `version` as that's special-cased
    // in Clap.
    #[clap(short, long = "version")]
    v: bool,

    /// A path to an individual XML file to index. The output is
    /// printed as HTML to stdout if only this option is used. The
    /// option can be given multiple times, with one path each
    /// time. This option was added just for testing; normally you
    /// would just provide the base_path to the repository instead.
    #[clap(long)]
    path: Option<Vec<PathBuf>>,

    /// Generate *only* the `README.html` file. This has better
    /// layout but doesn't work for viewing on GitLab (and may not
    /// work for GitHub either).The default is to generate both files.
    #[clap(long)]
    html: bool,

    /// Generate *only* the `README.md` file. This works for
    /// viewing on GitLab but has somewhat broken layout. The default
    /// is to generate both files.
    #[clap(long)]
    md: bool,

    /// Add a footer with a timestamp ("Last updated") to the index
    /// files. Note: this causes every run to create modified files
    /// that will be commited even when there were no actual changes,
    /// thus probably not what you want!
    #[clap(long, short)]
    timestamp: bool,

    /// Write the index files (and commit them if requested) even if
    /// some files had errors and thus won't be indexed; the errors
    /// are written in a section at the top of the index files,
    /// though. The same errors are still printed to stderr, and
    /// reported as exit code 1, too, though--see
    /// `--ok-on-written-errors` to change that.
    #[clap(long, short)]
    write_errors: bool,

    /// If used together with `--write-errors`, does use exit code 0
    /// even if there were errors that were written to the index
    /// files. Errors are still also written to stderr, though--see
    /// `--silent-on-written-errors` to change that.
    #[clap(long, short)]
    ok_on_written_errors: bool,

    /// If used together with `--write-errors`, exits with exit code 0
    /// and does not print any errors to stderr if there are errros
    /// that are written to the index files (it can still give some
    /// other errors, though, like being unable to run Git).
    #[clap(long, short)]
    silent_on_written_errors: bool,

    /// Open the generated `README.html` file in a web browser.
    /// Tries the browsers specified in the `BROWSER` environment
    /// variable (split on ':' into program names or paths (on macOS
    /// don't pass paths into `/Applications`, just give the
    /// application name; you could use paths to scripts)), otherwise
    /// `sensile-browser`, `firefox`, `chromium`, `chrome`, and on
    /// macOS `safari`. Fails if none worked. Note: only opens the
    /// file if it was actually written to (i.e. when there were no
    /// errors or `--write-errors` was given).
    #[clap(long)]
    open: bool,

    /// Same as `--open` but only opens a browser if the file has
    /// changed since the last Git commit to it.
    #[clap(long)]
    open_if_changed: bool,

    /// Git pull from the default remote into the local Git checkout
    /// before creating the index files.
    #[clap(long)]
    pull: bool,

    /// Do not add and commit the output files to the Git
    /// repository. It's better to let xmlhub-indexer do that (the
    /// default) rather than doing it manually, since it adds its
    /// version information to the commit message and later
    /// invocations of it check whether it needs upgrading.
    #[clap(long, short)]
    no_commit: bool,

    /// Push the local Git changes to the default remote after
    /// committing. Does nothing if the `--no-commit` option was
    /// given, or if there were no changes.
    #[clap(long)]
    push: bool,

    /// Do not run external processes like git or browsers,
    /// i.e. ignore all the options asking to do so. Instead just say
    /// on stderr what would be done. Still writes to the output
    /// files, though.
    #[clap(long)]
    dry_run: bool,

    /// Do not check the program version against versions specified in
    /// the automatic commit messages in the xmlhub repo. Only use if
    /// you know what you're doing.
    #[clap(long)]
    no_version_check: bool,

    /// The path to the base directory of the Git checkout of the XML
    /// Hub; it is an error if this is omitted and no --paths option
    /// was given. If given, writes the index as `README.html` and
    /// `README.md` files into this directory (otherwise the HTML
    /// variant is printed to standard output).
    base_path: Option<PathBuf>,
}

// =============================================================================
// Description of the valid metadata attributes and how they are
// parsed and displayed. First, the definition of the data types for
// the metadata (`struct`, `enum`, `impl`), then, using those, the
// actual description in `METADATA_SPECIFICATION`.

/// An attribute name is a string that identifies an attribute. The
/// string is in the canonical casing as it should be shown in
/// metadata listings in the HTML/Markdown output. To try to avoid
/// making mistakes, we define a wrapper struct `AttributeName` to
/// make it clear everywhere whether we're having a string in
/// canonical casing or not.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
struct AttributeName(&'static str);

impl AttributeName {
    /// Get the actual attribute name string
    fn as_str(self) -> &'static str {
        self.0
    }
}

/// Specifies whether an attribute is required
#[derive(Debug, PartialEq, Clone, Copy)]
enum AttributeNeed {
    Optional,
    /// Not "NA", nor the empty string / space only, and for lists not
    /// the empty list (even a list of empty elements like ", , ," is
    /// not OK)
    Required,
}

/// Specifies how an attribute value should be treated
#[derive(Debug, PartialEq, Clone, Copy)]
enum AttributeKind {
    String {
        /// Whether to convert groups of any kind of whitespace
        /// (spaces, tabs, newlines) to a single space. I.e. this
        /// strips space based "markup" if true. Note that in indexes,
        /// values are normalized anyway, so this matters only for the
        /// display in the file info boxes. (StringList items (the
        /// case below) are always normalized btw.)
        normalize_whitespace: bool,
        /// Whether to automatically create links of http and https
        /// URLs
        autolink: bool,
    },
    StringList {
        /// This is the separator as used between list items, in the
        /// XML files within the <!-- --> parts; e.g. if the items are
        /// separated by spaces, give " ", if separated by commas, give
        /// ",". This does not determine what's used for the HTML
        /// formatting; for that, see the `to_html` method on
        /// AttributeValue.
        separator: &'static str,
        /// Whether to automatically create links of http and https
        /// URLs
        autolink: bool,
    },
}

impl AttributeKind {
    fn is_list(&self) -> bool {
        match self {
            AttributeKind::String {
                normalize_whitespace: _,
                autolink: _,
            } => false,
            AttributeKind::StringList {
                separator: _,
                autolink: _,
            } => true,
        }
    }
}

/// Whether an index should be created
#[derive(Debug, PartialEq, Clone, Copy)]
enum AttributeIndexing {
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

/// All metainformation on an
#[derive(Debug)]
struct AttributeSpecification {
    key: AttributeName,
    need: AttributeNeed,
    kind: AttributeKind,
    indexing: AttributeIndexing,
}

/// Description of the metadata attributes, what they must contain,
/// and how they are indexed. The order of entries here is also the
/// same order used for showing the extracted info in the info boxes
/// in the index pages.
const METADATA_SPECIFICATION: &[AttributeSpecification] = {
    &[
        AttributeSpecification {
            key: AttributeName("Keywords"),
            need: AttributeNeed::Required,
            kind: AttributeKind::StringList {
                separator: ",",
                autolink: true,
            },
            indexing: AttributeIndexing::Index {
                first_word_only: false,
                use_lowercase: true,
            },
        },
        AttributeSpecification {
            key: AttributeName("Version"),
            need: AttributeNeed::Required,
            kind: AttributeKind::String {
                normalize_whitespace: false,
                autolink: true,
            },
            indexing: AttributeIndexing::Index {
                first_word_only: false,
                use_lowercase: false,
            },
        },
        AttributeSpecification {
            key: AttributeName("Packages"),
            need: AttributeNeed::Required,
            kind: AttributeKind::StringList {
                separator: ",",
                autolink: true,
            },
            indexing: AttributeIndexing::Index {
                first_word_only: true,
                use_lowercase: false,
            },
        },
        AttributeSpecification {
            key: AttributeName("Description"),
            need: AttributeNeed::Optional,
            kind: AttributeKind::String {
                normalize_whitespace: false,
                autolink: true,
            },
            indexing: AttributeIndexing::NoIndex,
        },
        AttributeSpecification {
            key: AttributeName("Comments"),
            need: AttributeNeed::Optional,
            kind: AttributeKind::String {
                normalize_whitespace: false,
                autolink: true,
            },
            indexing: AttributeIndexing::NoIndex,
        },
        AttributeSpecification {
            key: AttributeName("Citation"),
            need: AttributeNeed::Optional,
            kind: AttributeKind::String {
                normalize_whitespace: false,
                autolink: true,
            },
            indexing: AttributeIndexing::NoIndex,
        },
        AttributeSpecification {
            key: AttributeName("DOI"),
            need: AttributeNeed::Optional,
            kind: AttributeKind::String {
                normalize_whitespace: false,
                autolink: true,
            },
            indexing: AttributeIndexing::Index {
                first_word_only: false,
                use_lowercase: false,
            },
        },
        AttributeSpecification {
            key: AttributeName("Contact"),
            need: AttributeNeed::Required,
            kind: AttributeKind::String {
                normalize_whitespace: false,
                autolink: true,
            },
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
    static ref METADATA_KEY_POSITION: BTreeMap<AttributeName, usize> = METADATA_SPECIFICATION
        .iter()
        .enumerate()
        .map(|(i, spec)| (spec.key, i))
        .collect();
}

// =============================================================================
// Data structures to hold an attribute value, and the whole set of
// values for a file after their extraction from it, as well as
// operations (`impl` blocks) including parsing that information from
// strings and formatting the information as HTML.

/// An attribute value: either a string, a list of strings, or not
/// present.
#[derive(Debug)]
enum AttributeValue {
    String { value: String, autolink: bool },
    StringList { value: Vec<String>, autolink: bool },
    NA,
}

impl AttributeValue {
    /// Parse an input into the representation required by the given
    /// AttributeSpecification (like, a single string or
    /// lists). Returns an error if it couldn't do that, which happens
    /// if the input is only whitespace but a value is required by the
    /// spec.
    fn from_str_and_spec(val: &str, spec: &AttributeSpecification) -> Result<Self> {
        if val.is_empty() || val == "NA" {
            match spec.need {
                AttributeNeed::Optional => Ok(AttributeValue::NA),
                AttributeNeed::Required => {
                    bail!(
                        "attribute {:?} requires {}, but none given",
                        spec.key,
                        if spec.kind.is_list() {
                            "values"
                        } else {
                            "a value"
                        }
                    )
                }
            }
        } else {
            match spec.kind {
                AttributeKind::String {
                    normalize_whitespace,
                    autolink,
                } => {
                    let value = val.trim();
                    let value = if normalize_whitespace {
                        util::normalize_whitespace(value)
                    } else {
                        value.into()
                    };
                    Ok(AttributeValue::String { value, autolink })
                }
                AttributeKind::StringList {
                    separator,
                    autolink,
                } => {
                    // (Note: there is no need to replace '\n' with ' '
                    // in `val` first, because the trim will remove
                    // those around values, and normalize_whitespace will
                    // replace those within keys, too.)
                    let vals: Vec<String> = val
                        .split(separator)
                        .map(|s| util::normalize_whitespace(s.trim()))
                        .filter(|s| !s.is_empty())
                        .collect();
                    if vals.is_empty() {
                        match spec.need {
                            AttributeNeed::Optional => Ok(AttributeValue::NA),
                            AttributeNeed::Required => {
                                bail!(
                                    "values for attribute {:?} are required but missing",
                                    spec.key
                                )
                            }
                        }
                    } else {
                        Ok(AttributeValue::StringList {
                            value: vals,
                            autolink,
                        })
                    }
                }
            }
        }
    }

    /// Also works for single-value and unavailable attributes,
    /// returning a list of one or no entries, respectively. (`Cow`
    /// allows both sharing of existing vectors as well as holding new
    /// ones; that's just a performance feature, they can be used
    /// wherever a Vec or [] is required.)
    fn as_string_list(&self) -> Cow<[String]> {
        match self {
            AttributeValue::StringList { value, autolink: _ } => Cow::from(value.as_slice()),
            AttributeValue::NA => Cow::from(&[]),
            AttributeValue::String { value, autolink: _ } => Cow::from(vec![value.clone()]),
        }
    }

    /// Convert the value, or whole value list in the case of
    /// StringList, to HTML. This is used for the file info boxes for
    /// both .html and .md files. An `ASlice<Node>` is a list of
    /// elements (nodes), directly usable as the body (child elements)
    /// for another element.
    fn to_html(&self, html: &HtmlAllocator) -> Result<ASlice<Node>> {
        match self {
            AttributeValue::String { value, autolink } => {
                let softpre = SoftPre {
                    tabs_to_nbsp: Some(8),
                    autolink: *autolink,
                    line_separator: "\n",
                };
                softpre.format(value, html)?.to_aslice(html)
            }
            AttributeValue::StringList { value, autolink } => {
                let mut body = html.new_vec();
                let mut need_comma = false;
                for s in value {
                    if need_comma {
                        body.push(html.text(", ")?)?;
                    }
                    need_comma = true;
                    // Do not do SoftPre for string list items, only
                    // autolink if requested. Wrap in <q></q>.
                    if *autolink {
                        body.push(html.q([], ahtml::util::autolink(html, s)?)?)?;
                    } else {
                        body.push(html.q([], html.text(s)?)?)?;
                    }
                }
                Ok(body.as_slice())
            }
            AttributeValue::NA => html.i([], html.text("n.A.")?)?.to_aslice(html),
        }
    }
}

/// The metadata for one file, specified via XML comments in it. The
/// keys are the same as (or a subset of) those in
/// `METADATA_SPECIFICATION`.
#[derive(Debug)]
struct Metadata(BTreeMap<AttributeName, AttributeValue>);

impl Metadata {
    /// Retrieve the value for an attribute name.
    fn get(&self, key: AttributeName) -> Option<&AttributeValue> {
        self.0.get(&key).or_else(|| {
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
        let mut result: Vec<_> = METADATA_SPECIFICATION
            .iter()
            .map(|spec| (spec.key, None))
            .collect();
        for (key, attval) in &self.0 {
            let i = METADATA_KEY_POSITION[key];
            result[i].1 = Some(attval);
        }
        result
    }

    /// An HTML table with all metadata.
    fn to_html(&self, html: &HtmlAllocator) -> Result<AId<Node>> {
        let mut table_body = html.new_vec();
        for (key, opt_attval) in self.sorted_entries() {
            let attval_html = if let Some(attval) = opt_attval {
                attval.to_html(html)?
            } else {
                // Entry is missing in the file; show that fact.
                // (Also report that top-level as a warning? That
                // would be a bit ugly to implement.)
                html.i(
                    [
                        att("style", "color: red;"),
                        att(
                            "title",
                            format!(
                                "The XML comment for {key:?} is completely missing \
                                 in this file, perhaps because of an oversight."
                            ),
                        ),
                    ],
                    html.text("entry missing")?,
                )?
                .to_aslice(html)?
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
                        html.i([], [html.text(key.as_str())?, html.text(":")?])?,
                    )?,
                    html.td([att("class", "metadata_value")], attval_html)?,
                ],
            )?)?;
        }
        html.table([att("class", "metadata"), att("border", 0)], table_body)
    }
}

/// The whole information on one file
#[derive(Debug)]
struct FileInfo {
    id: usize,
    path: RelPathWithBase,
    metadata: Metadata,
}

// For FileInfo to go into a BTreeSet (`BTreeSet<&FileInfo>` further
// below), it needs to be orderable. Only `id` is relevant for that
// (and the other types have no Ord implementation), thus write
// implementations manually:
impl PartialOrd for FileInfo {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.id.partial_cmp(&other.id)
    }
}
impl Ord for FileInfo {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.id.cmp(&other.id)
    }
}
impl PartialEq for FileInfo {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for FileInfo {}

impl FileInfo {
    /// Show in a box with a table of the metadata
    fn to_box_html(
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
                                    [att(
                                        "href",
                                        // (This would need path
                                        // calculation if the index files
                                        // weren't written to the
                                        // top-level directory)
                                        self.path.rel_path(),
                                    )],
                                    html.text(file_path_or_name)?,
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
                ],
            )?,
        )
    }
}

// =============================================================================
// An abstraction of document sections:
//
// * that can be formatted for an HTML file or for a Markdown file
//   with embedded HTML;
// * that a table of contents can be built from (showing and linking the
//   (possibly nested) subsections).

/// A section consists of a (optional) section title, an optional
/// intro (that could be the only content), and a list of subsections
/// which could be empty. The toplevel section will not have a title,
/// but just a list of subsections. If `in_red` is true, will show the
/// title in red if possible.
struct Section {
    in_red: bool,
    title: Option<String>,
    intro: Option<AId<Node>>,
    subsections: Vec<Section>,
}

/// The list of section numbers (like "1.3.2") to identify a
/// particular subsection, used for naming them and linking from the
/// table of contents.
struct NumberPath {
    numbers: Vec<usize>,
}

impl NumberPath {
    fn empty() -> Self {
        Self {
            numbers: Vec::new(),
        }
    }
    /// Make a new path by adding an id to the end of the current one.
    fn add(&self, number: usize) -> Self {
        let mut numbers = self.numbers.clone();
        numbers.push(number);
        Self { numbers }
    }
    /// Gives e.g. `3` for a 3-level deep path
    fn level(&self) -> usize {
        self.numbers.len()
    }
    /// Gives e.g. `"1.3.2"`
    fn to_string(&self) -> String {
        self.numbers
            .iter()
            .map(|number| format!("{number}"))
            .collect::<Vec<String>>()
            .join(".")
    }
}

impl Section {
    /// Build a table of contents
    fn to_toc_html(&self, number_path: NumberPath, html: &HtmlAllocator) -> Result<AId<Node>> {
        let title_node = if let Some(title) = &self.title {
            let number_path_string = number_path.to_string();
            let section_id = format!("section-{number_path_string}");
            html.a(
                [
                    att("class", "toc_entry"),
                    if self.in_red {
                        att("style", "color: red;")
                    } else {
                        None
                    },
                    att("href", format!("#{section_id}")),
                ],
                html.text(format!("{number_path_string} {title}"))?,
            )?
        } else {
            html.empty_node()?
        };
        let mut sub_nodes = html.new_vec();
        for (i, section) in self.subsections.iter().enumerate() {
            let id = i + 1;
            let sub_path = number_path.add(id);
            sub_nodes.push(section.to_toc_html(sub_path, html)?)?;
        }
        html.dl([], [html.dt([], title_node)?, html.dd([], sub_nodes)?])
    }

    /// Format the section for the inclusion in an HTML file
    fn to_html(&self, number_path: NumberPath, html: &HtmlAllocator) -> Result<ASlice<Node>> {
        let mut vec = html.new_vec();
        if let Some(title) = &self.title {
            // Choose the html method for the current nesting level by
            // indexing into the list of them, referring to the
            // methods via function reference syntax
            // (e.g. `html.h2(..)` has to be called right away,
            // `html.h2` alone is not valid, but `HtmlAllocator::h2`
            // gets a reference to the h2 method without calling it,
            // but it is now actually a reference to a normal
            // function, hence further down, `element` has to be
            // called with `html` as a normal function argument
            // instead).
            let elements = [
                HtmlAllocator::h1,
                HtmlAllocator::h2,
                HtmlAllocator::h3,
                HtmlAllocator::h4,
                HtmlAllocator::h5,
                HtmlAllocator::h6,
            ];
            let mut level = number_path.level();
            let max_level = elements.len() - 1;
            if level > max_level {
                level = max_level;
            }
            let element = elements[level];

            let number_path_string = number_path.to_string();
            let section_id = format!("section-{number_path_string}");
            vec.push(html.a([att("name", &section_id)], [])?)?;
            vec.push(element(
                html,
                [
                    att("id", section_id),
                    if self.in_red {
                        att("style", "color: red;")
                    } else {
                        None
                    },
                ],
                // Prefix the path to the title; don't try to use CSS
                // as it won't make it through Markdown.
                html.text(format!("{number_path_string} {title}"))?,
            )?)?;
        }

        if let Some(node) = self.intro {
            vec.push(node)?;
        }

        for (i, section) in self.subsections.iter().enumerate() {
            let id = i + 1;
            let sub_path = number_path.add(id);
            vec.push(html.div([], section.to_html(sub_path, html)?)?)?;
        }

        Ok(vec.as_slice())
    }

    /// Format the section for the inclusion in a markdown file
    fn to_markdown(&self, number_path: NumberPath, html: &HtmlAllocator) -> Result<String> {
        let mut result = String::new();
        if let Some(title) = &self.title {
            let number_path_string = number_path.to_string();
            let section_id = format!("section-{number_path_string}");
            let mut num_hashes = number_path.level() + 1;
            if num_hashes > 6 {
                num_hashes = 6
            }
            for _ in 0..num_hashes {
                result.push('#');
            }
            result.push(' ');
            // Add an anchor for in-page links; use both available
            // approaches, the older "name" and the newer "id"
            // approach, hoping that at least one gets through
            // GitLab's formatting.
            result.push_str(
                &html
                    .a([att("name", &section_id), att("id", &section_id)], [])?
                    .to_html_fragment_string(html)?,
            );
            result.push_str(&number_path_string);
            result.push(' ');
            // Should we use HTML to try to make this red if
            // `self.in_red`? But GitLab drops it anyway, and there's
            // risk of messing up the title display.
            result.push_str(title);
            result.push_str("\n\n");
        }

        if let Some(node) = self.intro {
            result.push_str(&node.to_html_fragment_string(html)?);
            result.push_str("\n\n");
        }

        for (i, section) in self.subsections.iter().enumerate() {
            let id = i + 1;
            let sub_path = number_path.add(id);
            result.push_str(&section.to_markdown(sub_path, html)?);
        }

        Ok(result)
    }
}

// =============================================================================
/// An abstraction for folders and files, to collect the paths
/// reported by Git into, and then to map to nested
/// `Section`s. Contained files/folders are stored sorted by the name
/// of the file/folder inside this Folder; files and folders are
/// stored in separate struct fields, because their formatting will
/// also end up separately in a `Section` (files go to the intro,
/// folders to the subsections).
struct Folder<'f> {
    files: BTreeMap<String, &'f FileInfo>,
    folders: BTreeMap<String, Folder<'f>>,
}

impl<'f> Folder<'f> {
    fn new() -> Self {
        Self {
            files: BTreeMap::new(),
            folders: BTreeMap::new(),
        }
    }

    // This is just a helper method that recursively calls itself; see
    // `add` for the relevant wrapper method.
    fn add_(&mut self, segments: &[&str], file: &'f FileInfo) -> Result<()> {
        match segments {
            [] => unreachable!(),
            [segment, rest @ ..] => {
                if rest.is_empty() {
                    // `segment` being the last of the segments means
                    // that it represents the file name of the XML
                    // file itself
                    if let Some(oldfile) = self.files.get(*segment) {
                        bail!("duplicate file: {file:?} already entered as {oldfile:?}")
                    } else {
                        self.files.insert(segment.to_string(), file);
                    }
                } else {
                    if let Some(folder) = self.folders.get_mut(*segment) {
                        folder.add_(rest, file)?;
                    } else {
                        let mut folder = Folder::new();
                        folder.add_(rest, file)?;
                        self.folders.insert(segment.to_string(), folder);
                    }
                }
            }
        }
        Ok(())
    }

    /// Add a `FileInfo` to the right place in the `Folder` hierarchy
    /// according to the `FileInfo`'s `rel_path`, which is split into
    /// path segments.
    fn add(&mut self, file: &'f FileInfo) -> Result<()> {
        let segments: Vec<&str> = file.path.rel_path().split('/').collect();
        self.add_(&segments, file)
    }

    /// Convert to nested `Section`s.
    fn to_section(&self, title: Option<String>, html: &HtmlAllocator) -> Result<Section> {
        // Create and then fill in a vector of boxes which we'll use
        // as the body for a `div` HTML element; this vector is a
        // custom vector implementation that allocates its storage
        // space from the `HtmlAllocator` in `html`, that's why we
        // allocate it via the `new_vec` method and not via
        // `Vec::new()`.
        let mut file_info_boxes = html.new_vec();
        for (file_name, file_info) in &self.files {
            file_info_boxes.push(file_info.to_box_html(&html, "box", file_name)?)?;
        }

        // Using a normal vector here.
        let mut subsections = Vec::new();
        for (folder_name, folder) in &self.folders {
            // Append a '/' to folder_name to indicate that those are
            // folder names
            subsections.push(folder.to_section(Some(format!("{folder_name}/")), html)?);
        }

        Ok(Section {
            in_red: false,
            title,
            intro: Some(html.div([], file_info_boxes)?),
            subsections,
        })
    }
}

// =============================================================================
// Error reporting: this program does not stop but continues
// processing when it encounters errors, collecting them and then in
// the end reporting them all (both on the command line and in the
// output page).

/// An error report with all errors that happened while processing a
/// particular file.
#[derive(Debug)]
struct FileErrors {
    path: RelPathWithBase,
    errors: Vec<String>,
}

impl FileErrors {
    /// Returns `<dt>..<dd>..` (definition term / definition data)
    /// pairs to be used in a `<dl>..</dl>` (definition list).
    fn to_html(&self, html: &HtmlAllocator) -> Result<Flat<Node>> {
        const SOFT_PRE: SoftPre = SoftPre {
            tabs_to_nbsp: Some(4),
            autolink: true,
            line_separator: "\n",
        };
        let mut ul_body = html.new_vec();
        for error in &self.errors {
            ul_body.push(html.li([], SOFT_PRE.format(error, html)?)?)?;
        }
        let dt = html.dt(
            [],
            [
                html.text("For ")?,
                html.a(
                    [att("href", self.path.rel_path())],
                    html.text(self.path.rel_path())?,
                )?,
                html.text(":")?,
            ],
        )?;
        let dd = html.dd([], html.ul([], ul_body)?)?;
        Ok(Flat::Two(dt, dd))
    }

    /// Print as plaintext, for error reporting to stderr.
    fn print_plain<O: Write>(&self, out: &mut O) -> Result<()> {
        writeln!(out, "    For {:?}:", self.path.rel_path())?;
        for error in &self.errors {
            let lines: Vec<&str> = error.split('\n').collect();
            writeln!(out, "      * {}", lines[0])?;
            for line in &lines[1..] {
                writeln!(out, "        {}", line)?;
            }
        }
        Ok(())
    }
}

// =============================================================================
// Parsing and printing functionality, including the main function (the program
// entry point) at the bottom.

/// Parse all XML comments from above the first XML opening element
/// out of one file as `Metadata`.
fn parse_comments(comments: &[String]) -> Result<Metadata, Vec<String>> {
    let spec_by_lowercase_key: BTreeMap<String, &AttributeSpecification> = METADATA_SPECIFICATION
        .iter()
        .map(|spec| (spec.key.as_str().to_lowercase(), spec))
        .collect();
    let mut unseen_specs_by_lowercase_key = spec_by_lowercase_key.clone();
    let mut map: BTreeMap<AttributeName, AttributeValue> = BTreeMap::new();

    // Collect all errors instead of stopping at the first one.
    let mut errors: Vec<String> = Vec::new();
    for (i, comment) in comments.iter().enumerate() {
        // Using a function without arguments and calling it right
        // away to capture the result (Ok or Err).
        let result = (|| {
            if let Some((key_, value)) = comment.split_once(":") {
                let lc_key = key_.trim().to_lowercase();
                let value = value.trim();

                if let Some(spec) = spec_by_lowercase_key.get(&lc_key) {
                    unseen_specs_by_lowercase_key.remove(&lc_key);
                    if map.contains_key(&spec.key) {
                        bail!("duplicate entry for key {lc_key:?}")
                    } else {
                        let value = AttributeValue::from_str_and_spec(value, spec)?;
                        map.insert(spec.key, value);
                    }
                } else {
                    bail!("unknown key {lc_key:?} given")
                }
            } else {
                bail!("comment does not start with a keyword name and ':'")
            }
            Ok(())
        })()
        .with_context(|| anyhow!("XML comment no. {}", i + 1));
        if let Err(e) = result {
            errors.push(format!("{e:#}"));
        }
    }

    let missing: Vec<AttributeName> = unseen_specs_by_lowercase_key
        .into_values()
        .filter_map(|spec| {
            // Do not report as missing if it's optional
            if spec.need == AttributeNeed::Optional {
                None
            } else {
                Some(spec.key)
            }
        })
        .collect();
    if !missing.is_empty() {
        // Show just the names, not the AttributeName wrappers
        let missing_strings: Vec<&'static str> = missing.iter().map(|key| key.as_str()).collect();
        errors.push(format!(
            "attributes with these names are missing: {missing_strings:?}",
        ));
    }

    if errors.is_empty() {
        Ok(Metadata(map))
    } else {
        Err(errors)
    }
}

struct KeyvaluePreparation {
    first_word_only: bool,
    use_lowercase: bool,
}

impl KeyvaluePreparation {
    fn prepare_keyvalue(&self, keyvalue: &str) -> String {
        let mut keyvalue_prepared: String = util::normalize_whitespace(keyvalue.trim());
        // ^ Should we keep newlines instead, and then SoftPre for the
        // display? Probably not.
        if self.first_word_only {
            keyvalue_prepared = keyvalue_prepared
                .split(' ')
                .next()
                .expect("keyvalue is not empty")
                .into();
        }
        if self.use_lowercase {
            keyvalue_prepared = keyvalue_prepared.to_lowercase()
        }
        keyvalue_prepared
    }
}

/// Build an index over all files for one particular attribute name (`attribute_key`).
fn build_index_section(
    html: &HtmlAllocator,
    attribute_key: AttributeName,
    keyvalue_normalization: KeyvaluePreparation,
    file_infos: &[FileInfo],
) -> Result<Section> {
    // Build an index by the value for attribute_key (lower-casing the
    // key values for consistency if use_lowercase is true). The index
    // maps from key value to a set of all `FileInfo`s for that
    // value. The BTreeMap keeps the key values sorted alphabetically,
    // which is nice so we don't have to sort those afterwards.
    let mut file_infos_by_keyvalue: BTreeMap<String, BTreeSet<&FileInfo>> = BTreeMap::new();

    for file_info in file_infos {
        if let Some(attribute_value) = file_info.metadata.get(attribute_key) {
            for keyvalue in attribute_value.as_string_list().iter() {
                file_infos_by_keyvalue
                    .insert_value(keyvalue_normalization.prepare_keyvalue(keyvalue), file_info);
            }
        }
    }

    // The contents of the section, i.e. the list of all keyvalues and
    // the files for the respective keyvalue.
    let mut body = html.new_vec();
    for (keyvalue, file_infos) in &file_infos_by_keyvalue {
        // Output the key value
        body.push(html.dt(
            // The first list passed to HTML constructor methods like
            // `dt` is holding attributes, the second the child
            // elements (but a single child element can also be passed
            // without putting it into a list). The `?` is needed to
            // handle errors, because those method calls can fail,
            // either when they detect nesting of HTML elements that
            // doesn't conform to the HTML standard, or when the
            // allocator is running against the allocation limit that
            // was provided to `HtmlAllocator::new`.
            [att("class", "key_dt")],
            html.strong(
                [att("class", "key")],
                html.i([], html.q([], html.text(keyvalue)?)?)?,
            )?,
        )?)?;

        // Output all the files for that key value, sorted by path.
        let mut sorted_file_infos: Vec<&FileInfo> = file_infos.iter().copied().collect();
        sorted_file_infos.sort_by_key(|fileinfo| fileinfo.path.full_path());
        let mut dd_body = html.new_vec();
        for file_info in sorted_file_infos {
            // Show the path, and link to the actual XML file, but
            // also provide a link to the box with the extracted
            // metainfo further up the page.
            let rel_path = file_info.path.rel_path();
            let path_with_two_links_html = html.div(
                [att("class", "file_link")],
                [
                    html.a(
                        [
                            att("href", format!("#box-{}", file_info.id)),
                            att("title", "Jump to info box"),
                        ],
                        html.text(INFO_SYMBOL)?,
                    )?,
                    html.nbsp()?,
                    html.a(
                        [att("href", rel_path), att("title", "Open the file")],
                        html.text(rel_path)?,
                    )?,
                ],
            )?;

            dd_body.push(path_with_two_links_html)?;
        }
        body.push(html.dd(
            [att("class", "key_dd")],
            html.div([att("class", "key_dd")], dd_body)?,
        )?)?;
    }

    Ok(Section {
        in_red: false,
        title: Some(attribute_key.as_str().into()),
        intro: Some(html.dl([att("class", "key_dl")], body)?),
        subsections: vec![],
    })
}

/// Create a <div>&nbsp;<br>...</div> occupying some amount of
/// whitespace; useful at the end of the document to ensure that
/// document-internal links (e.g. from the table of contents) always
/// allows the document to be moved so that the link target is at the
/// top of the window.
fn empty_space_element(number_of_br_elements: usize, html: &HtmlAllocator) -> Result<AId<Node>> {
    let mut brs = html.new_vec();
    for _ in 0..number_of_br_elements {
        brs.push(html.nbsp()?)?;
        brs.push(html.br([], [])?)?;
    }
    html.div([], brs)
}

/// CSS style information; only useful for the .html file, not
/// included in the .md file as GitLab will ignore it anyway when
/// formatting that file.

const FILEINFO_PATH_BGCOLOR: &str = "#cec7f2";
const FILEINFO_METADATA_BGCOLOR: &str = "#e3e7ff";

fn css_styles() -> String {
    [
        "
/* make sections/subsections stand out more */
h2 {
  margin-top: 40px;
}

h3 {
  border-bottom: 2px solid #407cd9;
  margin-top: 40px;
}

/* a TABLE */
.fileinfo {
  border-spacing: 0px;
  margin-bottom: 20px; /* should instead use a grid something so that fileinfo is reusable */
}
/* a TD */
.fileinfo_path {
  background-color: ",
        FILEINFO_PATH_BGCOLOR,
        ";
  font-weight: bold;
}
/* a TR */
.fileinfo_metadata {
  background-color: ",
        FILEINFO_METADATA_BGCOLOR,
        ";
}
/* a TD */
.metadata_key {
  vertical-align: top;
  text-align: right;
  font-style: italic;
  padding-right: 6px;
  padding-left: 2px;
  padding-top: 2px;
  padding-bottom: 2px;
}
/* a TD */
.metadata_value {
  padding: 2px;
}
.key_dl {
}
.key_dt {
  margin-top: 1.5em;
  margin-bottom: 0.8em;
}
.key_dd {
}
/* a STRONG */
.key {
}
/* a DIV */
.file_link {
}
",
    ]
    .join("")
}

/// The value of this constant is generated by the `build.rs` program
/// during compilation. It is the output of running `git describe
/// --tags`.
const GIT_VERSION: &str = env!("GIT_DESCRIBE");

fn main() -> Result<()> {
    // Retrieve the command line options / arguments.
    let opts: Opts = Opts::from_args();

    let program_version: GitVersion<SemVersion> = GIT_VERSION
        .parse()
        .with_context(|| anyhow!("the git tag for the release version is not in a valid format"))?;

    if opts.v {
        println!("{PROGRAM_NAME} {program_version}");
        return Ok(());
    }

    let git_log_version_checker = GitLogVersionChecker {
        program_name: PROGRAM_NAME.into(),
        program_version,
    };

    // Define a macro to only run $body if opts.dry_run is false,
    // otherwise show $message instead.
    macro_rules! check_dry_run {
        { message: $message:expr, $body:expr } => {
            if opts.dry_run {
                let s: String = $message.into();
                eprintln!("--dry-run: would run: {s}");
            } else {
                $body;
            }
        }
    }

    let do_both = (!opts.html) && (!opts.md);
    let do_html = opts.html || do_both;
    let do_md = opts.md || do_both;

    // Try to get the paths from the `paths` option, if available,
    // otherwise read the files in a Git repo given by the base_path
    // option. Collect them as a vector of `RelPathWithBase` values,
    // each of which carries both a path to a base directory
    // (optional) and a relative path from there (if it contains no
    // base directory, the current working directoy is the base).
    let paths: Vec<RelPathWithBase> = if let Some(paths) = opts.path {
        paths
            .into_iter()
            .map(|p| RelPathWithBase::new(None, p))
            .collect()
    } else {
        // There were no `paths` given; instead get the base_path
        // argument, complain if it's missing.
        let base_path = opts.base_path.as_ref().ok_or_else(|| {
            anyhow!(
                "need the path to the XML Hub repository (or the --paths \
                 option). Run with --help for details."
            )
        })?;

        if !opts.no_version_check {
            // Verify that this is not an outdated version of the program.
            let found = git_log_version_checker
                .check_git_log(base_path, &[HTML_FILENAME, MD_FILENAME])
                .with_context(|| {
                    anyhow!(
                        "you should update your copy of the {PROGRAM_NAME} program. \
                     If you're sure you want to proceed anyway, use the \
                     --no-version-check option."
                    )
                })?;
            if found.is_none() {
                println!(
                    "Warning: could not find or parse {PROGRAM_NAME} version statements \
                 in the git log on the output files; this may mean that \
                 this is a fresh xmlhub Git repository, or something is messed up. \
                 This means that if {PROGRAM_NAME} is used from another computer, \
                 if its version is producing different output from this version \
                 then each will overwrite the changes from the other endlessly."
                );
            }
        }

        // Get the paths from running `git ls-files` inside the
        // directory at base_path, then ignore all files that don't
        // end in .xml
        let mut paths = vec![];
        check_dry_run! {
            message: "git ls-files",
            paths = git_ls_files(base_path)?
        }
        paths
            .into_iter()
            .filter(|path| {
                if let Some(ext) = path.extension() {
                    ext.eq_ignore_ascii_case("xml")
                } else {
                    false
                }
            })
            .collect()
    };

    // Carry out `git pull` if requested
    if let Some(base_path) = &opts.base_path {
        if opts.pull {
            check_dry_run! {
                message: "git pull",
                if !git(base_path, &["pull"])? {
                    bail!("git pull failed")
                }
            }
        }
    }

    // Map each file to the info extracted from it (or `FileErrors`
    // when there were errors), including path and an id, held in a
    // `FileInfo` struct. Generate the ids on the go for each of them
    // by `enumerate`ing the values (the enumeration number value is
    // passed as the `id` argument to the function given to `map`).
    // The id is used to refer to each item in document-local links in
    // the generated HTML/Markdown files.
    let fileinfo_or_errors: Vec<Result<FileInfo, FileErrors>> = paths
        .into_iter()
        .enumerate()
        .map(|(id, path)| -> Result<FileInfo, FileErrors> {
            // We're currently doing nothing with the `xmldoc` value
            // from `parse_xml_file` (which is the tree of all
            // elements, excluding the comments), thus prefixed with
            // an underscore to avoid the compiler warning about that.
            let (comments, _xmldoc) =
                parse_xml_file(&path.full_path()).map_err(|e| FileErrors {
                    path: path.clone(),
                    errors: vec![format!("{e:#}")],
                })?;
            let metadata = parse_comments(&comments).map_err(|errors| FileErrors {
                path: path.clone(),
                errors,
            })?;
            Ok(FileInfo { id, path, metadata })
        })
        .collect();

    // Partition fileinfo_or_errors into vectors with only the
    // successful and only the erroneous results.
    let (file_infos, file_errorss): (Vec<FileInfo>, Vec<FileErrors>) =
        fileinfo_or_errors.into_iter().partition_result();

    // Build the HTML fragments to use in the HTML page and the Markdown
    // file.

    // `HtmlAllocator` is an allocator for HTML elements (it manages memory
    // efficiently, and provides a method for each HTML element by its
    // name, e.g. `html.p(...)` creates a <p>...</p> element). The
    // number passed to `new` is the limit on the number of
    // allocations (a safety feature to limit damage when dealing with
    // attackers of web systems; irrelevant here, just choosing a
    // number large enough.) Rust allows underscores in numbers to
    // allow for better readability of large numbers.
    let html = HtmlAllocator::new(1_000_000);

    // Create all the sections making up the output file(s)

    // Create a Section with boxes with the metainfo for all XML
    // files, in a hierarchy reflecting the folder hierarchy where
    // they are.
    let file_info_boxes_section: Section = {
        // Temporarily create a folder hierarchy from all the paths,
        // then convert it to a Section.

        let mut folder = Folder::new();
        for file_info in &file_infos {
            folder.add(file_info).expect("no duplicates");
        }
        // This being the last expression in a { } block returns
        // (moves) its value to the `file_info_boxes_section`
        // variable outside.
        folder.to_section(Some("File info by folder".into()), &html)?
    };

    // Create all indices for those metadata entries for which their
    // specification says to index them. Each index is in a separate
    // `Section`.
    let index_sections: Vec<Section> = {
        let mut sections: Vec<Section> = Vec::new();
        for spec in METADATA_SPECIFICATION {
            match spec.indexing {
                AttributeIndexing::Index {
                    first_word_only,
                    use_lowercase,
                } => sections.push(build_index_section(
                    &html,
                    spec.key,
                    KeyvaluePreparation {
                        first_word_only,
                        use_lowercase,
                    },
                    &file_infos,
                )?),
                AttributeIndexing::NoIndex => (),
            }
        }
        sections
    };

    // Make a `Section` with all the errors if there are any
    let errors_section = if file_errorss.is_empty() {
        None
    } else {
        let mut vec = html.new_vec();
        for file_errors in &file_errorss {
            vec.push_flat(file_errors.to_html(&html)?)?;
        }
        Some(Section {
            in_red: true,
            title: Some("Errors".into()),
            intro: Some(html.dl([], vec)?),
            subsections: vec![],
        })
    };

    // Create a single section without a title, to enclose all the
    // other sections. This way, creating the table of contents and
    // conversion to HTML vs. Markdown works seamlessly.
    let toplevel_section = Section {
        in_red: false,
        title: None,
        intro: None,
        subsections: append(
            // This converts the optional `errors_section` from an
            // Option<Section> to a Vec<Section> that contains 0 or 1
            // sections.
            errors_section.into_iter().collect::<Vec<_>>(),
            // Always use the file_info_boxes_section and the index
            // sections.
            vec![
                Section {
                    in_red: false,
                    title: Some("Index by attribute".into()),
                    intro: None,
                    subsections: index_sections,
                },
                file_info_boxes_section,
            ],
        ),
    };

    // Some variables used in both the .html and .md documents
    let now = Local::now().to_rfc2822();
    let title = "XML Hub file index";
    let toc_html = toplevel_section.to_toc_html(NumberPath::empty(), &html)?;
    let generated_message = format!("auto-generated by {PROGRAM_NAME}, {PROGRAM_REPOSITORY}");

    // (For an explanation of the HTML creation syntax used below, see
    // the comment "The first list passed" further above.)

    // Make an intro, slightly differently depending on whether it is
    // for the .md or .html file.
    let intro = |making_md: bool| {
        html.div(
            [],
            [
                html.p(
                    [],
                    html.text(
                        "Welcome to the cEVO XML hub! This is a shared internal (private) \
                         repository for uploading XML files for BEAST2.",
                    )?,
                )?,
                html.p(
                    [],
                    [
                        html.text("To contribute XML files, see ")?,
                        html.a(
                            [att("href", format!("{CONTRIBUTE_FILE_NAME}.md"))],
                            html.text(CONTRIBUTE_FILE_NAME)?,
                        )?,
                        html.text(".")?,
                    ],
                )?,
                html.p(
                    [],
                    [
                        html.text("This is an index over all XML files, generated by ")?,
                        html.a([att("href", PROGRAM_REPOSITORY)], html.text(PROGRAM_NAME)?)?,
                        html.text(".")?,
                    ],
                )?,
                html.p(
                    [],
                    [html.text(format!(
                        "Click on the {INFO_SYMBOL} symbols to jump to the \
                         info box about that file, or on the link to open \
                         the XML file itself."
                    ))?],
                )?,
                if making_md {
                    html.p(
                        [],
                        html.small(
                            [],
                            html.text(format!(
                                "Note: if you \"git clone\" this repository, open the file \
                                 {HTML_FILENAME:?} instead, it has the same info already \
                                 formatted as HTML (and in fact has better formatting than \
                                 the view you're seeing here)."
                            ))?,
                        )?,
                    )?
                } else {
                    html.empty_node()?
                },
            ],
        )
    };

    // The contents for the README.html document
    let htmldocument = html.html(
        [],
        [
            html.head(
                [],
                [
                    html.meta(
                        [att("name", "generator"), att("content", &generated_message)],
                        [],
                    )?,
                    html.meta(
                        [att("name", "author"), att("content", &generated_message)],
                        [],
                    )?,
                    html.title([], html.text("Index - XML Hub")?)?,
                    html.style([], html.text(css_styles())?)?,
                ],
            )?,
            html.body(
                [],
                [
                    html.h1([], html.text(title)?)?,
                    intro(false)?,
                    html.h2([], html.text("Contents")?)?,
                    toc_html,
                    html.div([], toplevel_section.to_html(NumberPath::empty(), &html)?)?,
                    if opts.timestamp {
                        html.div(
                            [],
                            [
                                html.hr([], [])?,
                                html.p([], [html.text("Last updated: ")?, html.text(&now)?])?,
                            ],
                        )?
                    } else {
                        html.empty_node()?
                    },
                    empty_space_element(40, &html)?,
                ],
            )?,
        ],
    )?;

    // The contents for the README.md document
    let mddocument = {
        [
            vec![
                format!("<!-- NOTE: {generated_message}, do not edit manually! -->"),
                format!("# {title}"),
                intro(true)?.to_html_fragment_string(&html)?,
            ],
            vec![
                format!("## Contents"),
                toc_html.to_html_fragment_string(&html)?,
                toplevel_section.to_markdown(NumberPath::empty(), &html)?,
                empty_space_element(40, &html)?.to_html_fragment_string(&html)?,
            ],
            if opts.timestamp {
                vec![
                    format!("-------------------------------------------------------"),
                    format!("Last updated: {now}\n"),
                ]
            } else {
                vec![]
            },
        ]
        .flattened()
        .join("\n\n")
    };

    // The behaviour of the program in the face of errors depends on 3
    // command line options. Here's the logic that derives 3
    // behaviours from the 3 options (it's not 1:1). The Rust compiler
    // verifies that each of the 3 variables is set exactly once.
    let exit_code;
    let write_errors_to_stderr;
    let write_files;
    if file_errorss.is_empty() {
        exit_code = 0;
        write_errors_to_stderr = false;
        write_files = true;
    } else {
        if opts.write_errors {
            write_files = true;
            if opts.silent_on_written_errors {
                exit_code = 0;
                write_errors_to_stderr = false;
            } else {
                write_errors_to_stderr = true;
                if opts.ok_on_written_errors {
                    exit_code = 0;
                } else {
                    exit_code = 1;
                }
            }
        } else {
            exit_code = 1;
            write_errors_to_stderr = true;
            write_files = false;
        }
    }

    if write_errors_to_stderr {
        let mut out = stderr().lock();
        (|| -> Result<()> {
            write!(&mut out, "Indexing errors:\n")?;
            for file_errors in file_errorss {
                file_errors.print_plain(&mut out)?
            }
            Ok(())
        })()
        .context("writing to stderr")?;
    }

    let mut html_file_has_changed = false;
    if write_files {
        // Write the output files to the directory at `base_path` if
        // given.
        if let Some(base_path) = &opts.base_path {
            let mut written_files = Vec::new();
            if do_html {
                let mut path = base_path.clone();
                path.push(HTML_FILENAME);
                written_files.push(HTML_FILENAME);
                let mut out = BufWriter::new(File::create(&path)?);
                html.print_html_document(htmldocument, &mut out)?;
                out.flush()?;

                if opts.open_if_changed {
                    // Need to remember whether the file has changed
                    check_dry_run! {
                        message: "git diff",
                        html_file_has_changed = !git(
                            &base_path,
                            &["diff", "--no-patch", "--exit-code", "--", HTML_FILENAME],
                        )?
                    }
                }
            }
            if do_md {
                let mut path = base_path.clone();
                path.push(MD_FILENAME);
                written_files.push(MD_FILENAME);
                let mut out = File::create(&path)?;
                out.write_all(mddocument.as_bytes())?;
                out.flush()?;
            }

            // Commit files if not prevented by --no-commit, and any were written
            if (!opts.no_commit) && (!written_files.is_empty()) {
                // First check that there are no uncommitted changes
                let mut items = vec![];
                check_dry_run! {
                    message: "git status",
                    items = git_status(&base_path)?
                }
                let changed_items: Vec<_> = items
                    .iter()
                    .filter(|item| !written_files.contains(&item.path.as_str()))
                    .collect();
                if !changed_items.is_empty() {
                    bail!(
                        "won't run git commit due to uncommitted changes in {base_path:?} \
                         (you could use the --no-commit option): {changed_items:?}"
                    )
                }

                check_dry_run! {
                    message: "git add",
                    git(&base_path, &append(&["add", "-f", "--"], &written_files))?
                }

                let mut did_commit = true;
                check_dry_run! {
                    message: "git commit",
                    did_commit = git(
                        &base_path,
                        &append(
                            &[
                                "commit",
                                "-m",
                                &format!(
                                    "regenerate index file{} via {}",
                                    if written_files.len() > 1 { "s" } else { "" },
                                    git_log_version_checker.program_name_and_version()
                                ),
                                "--",
                            ],
                            &written_files,
                        ),
                    )?
                }

                if did_commit && opts.push {
                    check_dry_run! {
                        message: "git push",
                        git(&base_path, &["push"])?
                    }
                }
            }
        } else {
            html.print_html_document(htmldocument, &mut stdout().lock())?;
        }
    }

    // Open a web browser if appropriate
    if opts.open || (opts.open_if_changed && html_file_has_changed) {
        if write_files {
            if let Some(base_path) = &opts.base_path {
                // Hopefully all browsers take relative paths? Firefox
                // on Linux and macOS are OK, Safari (via open -a) as
                // well.  Otherwise would have to resolve base_path
                // with HTML_FILENAME pushed-on as an absolute path:
                // let mut path = base_path.clone();
                // path.push(HTML_FILENAME);
                // path.canonicalize().as_os_str()
                spawn_browser(base_path, &[HTML_FILENAME.as_ref()])?;
            } else {
                eprintln!(
                    "Note: not opening browser because no file was written because \
                     no BASE_PATH was given"
                );
            }
        } else {
            eprintln!(
                "Note: not opening browser because the files weren't written due \
                 to errors; specify --write-errors if you want to write and open \
                 the file with the errors"
            );
        }
    }

    exit(exit_code);
}
