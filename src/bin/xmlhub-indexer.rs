use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap, HashSet},
    fs::File,
    io::{stderr, stdout, BufWriter, Write},
    path::PathBuf,
    process::exit,
};

use ahtml::{att, AId, ASlice, Flat, HtmlAllocator, Node, Print, ToASlice};
use anyhow::{anyhow, bail, Context, Result};
use chrono::Local;
use clap::Parser;
use lazy_static::lazy_static;
use xmlhub_indexer::{
    browser::spawn_browser,
    git::{git, git_ls_files, git_status, RelPathWithBase},
    parse_xml::parse_xml_file,
    util::{append, flatten, get_by_key, normalize_whitespace, InsertValue},
};

const PROGRAM_NAME: &str = "xmlhub-indexer";
const PROGRAM_REPOSITORY: &str = "https://cevo-git.ethz.ch/cevo-resources/xmlhub-indexer";
const HTML_FILENAME: &str = "file_index.html";
const MD_FILENAME: &str = "file_index.md";

// =============================================================================
// Specification of the command line interface, using the `clap`
// library crate.

#[derive(clap::Parser, Debug)]
/// Build an index of the files in the (non-public) XML Hub of the
/// cEvo group at the D-BSSE, ETH Zurich.
struct Opts {
    /// The paths to the individual XML files to index. The output is
    /// printed as HTML to stdout if only this option is used. This
    /// option was added just for testing, normally, you would just
    /// provide the base_path to the repository instead
    #[clap(long)]
    paths: Option<Vec<PathBuf>>,

    /// Generate *only* the `file_index.html` file. This has better
    /// layout but doesn't work for viewing on GitLab (and may not
    /// work for GitHub either).The default is to generate both files.
    #[clap(long)]
    html: bool,

    /// Generate *only* the `file_index.md` file. This works for
    /// viewing on GitLab but has somewhat broken layout. The default
    /// is to generate both files.
    #[clap(long)]
    md: bool,

    /// Add a footer with a timestamp ("Last updated") to the index
    /// files. Note: this causes every run to create modified files
    /// that will be commited via `--commit` even when there were no
    /// actual changes!
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

    /// Open the generated `file_index.html` file in a web browser.
    /// Tries `sensible-browser`, the browsers specified in the
    /// `BROWSER` environment variable, which is split on ':' into
    /// program names or paths that are tried in order, `firefox`,
    /// `chromium` or `chrome`. Fails if none worked. Note: only opens
    /// the file if it was actually written to (i.e. when there were
    /// no errors or `--write-errors` was given).
    #[clap(long)]
    open: bool,

    /// Same as `--open` but only opens a browser if the file has
    /// changed since the last Git commit. (You may want to use this
    /// together with `--commit` so that on the next run it will not
    /// open the browser again.)
    #[clap(long)]
    open_if_changed: bool,

    /// Git pull from the default remote into the local Git checkout
    /// before creating the index files.
    #[clap(long)]
    pull: bool,

    /// Add and commit the output files to the Git repository.
    #[clap(long, short)]
    commit: bool,

    /// Push the local Git changes to the default remote after
    /// committing. Does nothing if the `--commit` option wasn't
    /// given, or if there were no changes.
    #[clap(long)]
    push: bool,

    /// The path to the base directory of the Git checkout of the XML
    /// Hub; it is an error if this is omitted and no --paths option
    /// was given. If given, writes the index as `file_index.html` and
    /// `file_index.md` files into this directory (otherwise the HTML
    /// variant is printed to standard output).
    base_path: Option<PathBuf>,
}

// =============================================================================
// Description of the valid metadata attributes and how they are
// parsed and displayed

/// Specifies whether an attribute is required
#[derive(Debug, PartialEq, Clone, Copy)]
enum AttributeNeed {
    Optional,
    NonEmpty,
}

/// Specifies how an attribute value should be treated
#[derive(Debug, PartialEq, Clone, Copy)]
enum AttributeKind {
    String {
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
        /// Whether to automatically link http and https URLs
        autolink: bool,
    },
}

/// Whether an index should be created
#[derive(Debug, PartialEq, Clone, Copy)]
enum AttributeIndexing {
    Index {
        /// Whether to convert the user-given values to lowercase for
        /// the index
        use_lowercase: bool,
    },
    NoIndex,
}

/// All metainformation on an
#[derive(Debug)]
struct AttributeSpecification {
    key: &'static str,
    need: AttributeNeed,
    kind: AttributeKind,
    indexing: AttributeIndexing,
}

/// Description of the metadata keys, what they must contain, and how
/// they are indexed. The order of entries here is also the same order
/// used for showing the extracted info in the info boxes in the index
/// pages.
const METADATA_SPECIFICATION: &[AttributeSpecification] = {
    &[
        AttributeSpecification {
            key: "Keywords",
            need: AttributeNeed::Optional,
            kind: AttributeKind::StringList {
                separator: " ",
                autolink: true,
            },
            indexing: AttributeIndexing::Index {
                use_lowercase: true,
            },
        },
        AttributeSpecification {
            key: "Version",
            need: AttributeNeed::NonEmpty,
            kind: AttributeKind::String { autolink: true },
            indexing: AttributeIndexing::Index {
                use_lowercase: false,
            },
        },
        AttributeSpecification {
            key: "Packages",
            need: AttributeNeed::NonEmpty,
            kind: AttributeKind::StringList {
                separator: " ",
                autolink: true,
            },
            indexing: AttributeIndexing::Index {
                use_lowercase: false,
            },
        },
        AttributeSpecification {
            key: "Description",
            need: AttributeNeed::Optional,
            kind: AttributeKind::String { autolink: true },
            indexing: AttributeIndexing::NoIndex,
        },
        AttributeSpecification {
            key: "Comments",
            need: AttributeNeed::Optional,
            kind: AttributeKind::String { autolink: true },
            indexing: AttributeIndexing::NoIndex,
        },
        AttributeSpecification {
            key: "Citation",
            need: AttributeNeed::Optional,
            kind: AttributeKind::String { autolink: true },
            indexing: AttributeIndexing::NoIndex,
        },
        AttributeSpecification {
            key: "DOI",
            need: AttributeNeed::Optional,
            kind: AttributeKind::String { autolink: true },
            indexing: AttributeIndexing::Index {
                use_lowercase: false,
            },
        },
        AttributeSpecification {
            key: "Contact",
            need: AttributeNeed::Optional,
            kind: AttributeKind::String { autolink: true },
            indexing: AttributeIndexing::Index {
                use_lowercase: false,
            },
        },
    ]
};

lazy_static! {
    // A mapping from a key name to its position; used for sorting the
    // user-provided metadata entries uniformly.
    static ref METADATA_KEY_POSITION: HashMap<&'static str, usize> = METADATA_SPECIFICATION
        .iter()
        .enumerate()
        .map(|(i, spec)| (spec.key, i))
        .collect();
}

// =============================================================================
// Data structures to hold an attribute value, and the whole set of
// values for a file after their extraction from it, as well as
// operations including formatting that information as HTML.

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
                AttributeNeed::NonEmpty => {
                    bail!("value for attribute {:?} is required but missing", spec.key)
                }
            }
        } else {
            match spec.kind {
                AttributeKind::String { autolink } => Ok(AttributeValue::String {
                    // Calling `normalize_whitespace` should only be
                    // useful for values used as index keys; but it
                    // also shouldn't hurt, as long as the text is not
                    // parsed as markdown or something.
                    value: normalize_whitespace(val.trim()),
                    autolink,
                }),
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
                        .map(|s| normalize_whitespace(s.trim()))
                        .filter(|s| !s.is_empty())
                        .collect();
                    if vals.is_empty() {
                        match spec.need {
                            AttributeNeed::Optional => Ok(AttributeValue::NA),
                            AttributeNeed::NonEmpty => {
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

    /// Convert to HTML, this is used for both .html and .md files. An
    /// ASlice<Node> is a list of elements (nodes), usable as the body
    /// (child elements) for another element.
    fn to_html(&self, html: &HtmlAllocator) -> Result<ASlice<Node>> {
        match self {
            AttributeValue::String { value, autolink } => {
                if *autolink {
                    ahtml::util::autolink(html, value)
                } else {
                    html.text(value)?.to_aslice(html)
                }
            }
            AttributeValue::StringList { value, autolink } => {
                let mut body = html.new_vec();
                let mut need_comma = false;
                for s in value {
                    if need_comma {
                        body.push(html.text(", ")?)?;
                    }
                    need_comma = true;
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
struct Metadata(HashMap<&'static str, AttributeValue>);

impl Metadata {
    /// Retrieve the value for a key. Panics if `key` is not defined
    /// in `METADATA_SPECIFICATION` (it is a program bug if this can
    /// happen; XXX replace with a type `AttributeName`?).
    fn get(&self, key: &str) -> Option<&AttributeValue> {
        self.0.get(key).or_else(|| {
            if get_by_key(METADATA_SPECIFICATION, |spec| &spec.key, &key).is_none() {
                panic!("invalid metadata key {key:?}")
            } else {
                None
            }
        })
    }

    /// The entries in the same order as given in
    /// `METADATA_SPECIFICATION`, with gaps where a key wasn't given
    /// in the file.
    fn sorted_entries(&self) -> Vec<(&'static str, Option<&AttributeValue>)> {
        let mut result: Vec<_> = METADATA_SPECIFICATION
            .iter()
            .map(|spec| (spec.key, None))
            .collect();
        for (key, attval) in &self.0 {
            let i = METADATA_KEY_POSITION[key];
            assert_eq!(&result[i].0, key);
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
                // Entry is missing in the file; show that fact. XX
                // also report that top-level as a warning? That would
                // be a bit ugly to implement.
                html.i([att("style", "color: red;")], html.text("entry missing")?)?
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
                        html.i([], [html.text(key)?, html.text(":")?])?,
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
/// but just a list of subsections.
struct Section {
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
                [att("id", section_id)],
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
                    .to_html_string(html)?,
            );
            result.push_str(&number_path_string);
            result.push(' ');
            result.push_str(title);
            result.push_str("\n\n");
        }

        if let Some(node) = self.intro {
            result.push_str(&node.to_html_string(html)?);
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
        let mut fileinfo_boxes = html.new_vec();
        for (file_name, fileinfo) in &self.files {
            fileinfo_boxes.push(fileinfo.to_box_html(&html, "box", file_name)?)?;
        }

        // Using a normal vector here.
        let mut subsections = Vec::new();
        for (folder_name, folder) in &self.folders {
            // Append a '/' to folder_name to indicate that those are
            // folder names
            subsections.push(folder.to_section(Some(format!("{folder_name}/")), html)?);
        }

        Ok(Section {
            title,
            intro: Some(html.div([], fileinfo_boxes)?),
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
    /// Returns <dt><dd> pairs to be used in a <dl> </dl>.
    fn to_html(&self, html: &HtmlAllocator) -> Result<Flat<Node>> {
        let mut ul_body = html.new_vec();
        for error in &self.errors {
            ul_body.push(html.li([], html.pre([], html.text(error)?)?)?)?;
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
        writeln!(out, "For {:?}:", self.path.rel_path())?;
        for error in &self.errors {
            let lines: Vec<&str> = error.split('\n').collect();
            writeln!(out, "  * {}", lines[0])?;
            for line in &lines[1..] {
                writeln!(out, "    {}", line)?;
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
    let spec_by_lowercase_key: HashMap<String, &AttributeSpecification> = METADATA_SPECIFICATION
        .iter()
        .map(|spec| (spec.key.to_lowercase(), spec))
        .collect();
    let mut unseen_specs_by_lowercase_key = spec_by_lowercase_key.clone();
    let mut map: HashMap<&'static str, AttributeValue> = HashMap::new();

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
                    if map.contains_key(spec.key) {
                        bail!("duplicate entry for key {lc_key:?}")
                    } else {
                        let value = AttributeValue::from_str_and_spec(value, spec)?;
                        map.insert(spec.key, value);
                    }
                } else {
                    bail!("unknown key {lc_key:?}")
                }
            } else {
                bail!("comment does not start with a keyword and ':'")
            }
            Ok(())
        })()
        .with_context(|| anyhow!("comment no. {}", i + 1));
        if let Err(e) = result {
            errors.push(format!("{e:#}"));
        }
    }

    let missing: Vec<&str> = unseen_specs_by_lowercase_key
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
        errors.push(format!("missing keys: {:?}", missing.as_slice()));
    }

    if errors.is_empty() {
        Ok(Metadata(map))
    } else {
        Err(errors)
    }
}

/// Build an index over all files for one particular attribute name (`attribute_key`).
fn build_index_section(
    html: &HtmlAllocator,
    attribute_key: &str,
    use_lowercase: bool,
    fileinfo_or_errors: &Vec<Result<FileInfo, FileErrors>>,
) -> Result<Section> {
    // Build an index by the value for attribute_key (lower-casing the
    // key values for consistency if use_lowercase is true). The index
    // maps from key value to a set of all FileInfo ids for that
    // value. The BTreeMap keeps the key values sorted alphabetically
    // as they are being inserted.
    let mut id_by_keyvalue: BTreeMap<String, HashSet<usize>> = BTreeMap::new();

    for fileinfo_or_error in fileinfo_or_errors {
        if let Ok(fileinfo) = fileinfo_or_error {
            if let Some(attribute_value) = fileinfo.metadata.get(attribute_key) {
                for keyvalue in attribute_value.as_string_list().iter() {
                    id_by_keyvalue.insert_value(
                        if use_lowercase {
                            keyvalue.to_lowercase()
                        } else {
                            keyvalue.into()
                        },
                        fileinfo.id,
                    );
                }
            }
        }
    }

    let mut body = html.new_vec();
    for (keyvalue, ids) in &id_by_keyvalue {
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
            html.strong([att("class", "key")], html.text(keyvalue)?)?,
        )?)?;

        // Output all the files for that key value
        let mut fileinfos_for_keyvalue: Vec<&FileInfo> = ids
            .iter()
            .map(|id| {
                fileinfo_or_errors[*id]
                    .as_ref()
                    .expect("was selected to be an OK")
            })
            .collect();
        fileinfos_for_keyvalue.sort_by_key(|fileinfo| fileinfo.path.full_path());
        let mut dd_body = html.new_vec();
        for fileinfo in fileinfos_for_keyvalue {
            // Show the path, and link to the actual XML file, but
            // also provide a link to the box with the extracted
            // metainfo further up the page.
            let rel_path = fileinfo.path.rel_path();
            let path_with_two_links_html = html.div(
                [att("class", "file_link")],
                [
                    html.a(
                        [
                            att("href", format!("#box-{}", fileinfo.id)),
                            att("title", "Jump to info box"),
                        ],
                        html.text("ℹ️")?,
                    )?,
                    html.nbsp()?,
                    html.a(
                        [att("href", rel_path), att("title", "Open the file")],
                        html.text(rel_path)?,
                    )?,
                    html.nbsp()?,
                    html.a(
                        [
                            att("href", format!("#box-{}", fileinfo.id)),
                            att("title", "Jump to info box"),
                        ],
                        html.text("⤴️")?,
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
        title: Some(attribute_key.into()),
        intro: Some(html.dl([att("class", "key_dl")], body)?),
        subsections: vec![],
    })
}

/// CSS style information; only useful for the .html file, not
/// included in the .md file as GitLab will ignore it anyway when
/// formatting that file.

const FILEINFO_PATH_BGCOLOR: &str = "#cec7f2";
const FILEINFO_METADATA_BGCOLOR: &str = "#e3e7ff";

fn css_styles() -> String {
    [
        "
/* a TABLE */
.fileinfo {
  border-spacing: 0px;
  margin-bottom: 20px; /* XX should instead use a grid something so that fileinfo is reusable */
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

fn main() -> Result<()> {
    // Retrieve the command line options / arguments.
    let opts: Opts = Opts::from_args();

    let do_both = (!opts.html) && (!opts.md);
    let do_html = opts.html || do_both;
    let do_md = opts.md || do_both;

    // Try to get the paths from the `paths` option, if available,
    // otherwise read the files in a Git repo given by the base_path
    // option. Collect them as a vector of `RelPathWithBase` values,
    // each of which carries both a path to a base directory
    // (optional) and a relative path from there (if it contains no
    // base directory, the current working directoy is the base).
    let paths: Vec<RelPathWithBase> = if let Some(paths) = opts.paths {
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
        // Get the paths from running `git ls-files` inside the
        // directory at base_path, then ignore all files that don't
        // end in .xml
        let paths = git_ls_files(base_path)?;
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
            if !git(base_path, &["pull"])? {
                bail!("git pull failed")
            }
        }
    }

    // Map each file to the info extracted from it (or `FileErrors`
    // when there were errors), including path and an id, held in a
    // `FileInfo` struct. Generate the ids on the go for each of them
    // by `enumerate`ing the values (the enumeration number value is
    // passed as the `id` argument to the function given to `map`).
    // The id is used to refer to each item in the index data structure
    // built from the `fileinfo_or_errors` further below (could also 
    // store `&` references in an index; but need some kind of id anyway 
    // for the document-local links in the HTML formatting).
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
            // `id` is also the index into `fileinfo_or_errors`
            Ok(FileInfo { id, path, metadata })
        })
        .collect();

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
        for fileinfo_or_error in &fileinfo_or_errors {
            if let Ok(fileinfo) = fileinfo_or_error {
                folder.add(fileinfo).expect("no duplicates");
            }
        }
        // This being the last expression in a { } block returns
        // (moves) its value to the `file_info_boxes_section`
        // variable outside.
        folder.to_section(Some("File info by folder".into()), &html)?
    };

    // Create all indices for those metadata entries for which their
    // specification says to index them. Each index is in a separate
    // `Section`.
    let index_sections: Vec<Section> =
        {
            let mut sections: Vec<Section> = Vec::new();
            for spec in METADATA_SPECIFICATION {
                match spec.indexing {
                    AttributeIndexing::Index { use_lowercase } => sections.push(
                        build_index_section(&html, spec.key, use_lowercase, &fileinfo_or_errors)?,
                    ),
                    AttributeIndexing::NoIndex => (),
                }
            }
            sections
        };

    // Create a single section without a title, to enclose all the
    // other sections. This way, creating the table of contents and
    // conversion to HTML vs. Markdown works seamlessly.
    let toplevel_section = Section {
        title: None,
        intro: None,
        subsections: vec![
            file_info_boxes_section,
            Section {
                title: Some("Index by attribute".into()),
                intro: None,
                subsections: index_sections,
            },
        ],
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
                    [
                        html.text("Also see the ")?,
                        html.a(
                            [att(
                                "href",
                                if making_md {
                                    "README.html"
                                } else {
                                    "README.md"
                                },
                            )],
                            html.text("README")?,
                        )?,
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
                if making_md {
                    html.p(
                        [],
                        html.text(format!(
                            "Note: GitLab limits what formatting a .md file can show. \
                             There is also the file {HTML_FILENAME:?} \
                             with the same information, \
                             if you clone this repository you could open \
                             that file directly in \
                             your browser to get better formatting."
                        ))?,
                    )?
                } else {
                    html.empty_node()?
                },
            ],
        )
    };

    // Extract all errors, make a `Section` if there are any
    let file_errorss: Vec<&FileErrors> = fileinfo_or_errors
        .iter()
        .filter_map(|v| if let Err(e) = v { Some(e) } else { None })
        .collect();
    let errors_section = if file_errorss.is_empty() {
        None
    } else {
        let mut vec = html.new_vec();
        for file_errors in &file_errorss {
            vec.push_flat(file_errors.to_html(&html)?)?;
        }
        Some(Section {
            title: Some("Errors".into()),
            intro: Some(html.dl([], vec)?),
            subsections: vec![],
        })
    };

    // The contents for the file_index.html document
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
                    if let Some(errors_section) = &errors_section {
                        html.div([], errors_section.to_html(NumberPath::empty(), &html)?)?
                    } else {
                        html.empty_node()?
                    },
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
                ],
            )?,
        ],
    )?;

    // The contents for the file_index.md document
    let mddocument = {
        flatten(&[
            vec![
                format!("<!-- NOTE: {generated_message}, do not edit manually! -->"),
                format!("# {title}"),
                intro(true)?.to_html_string(&html)?,
            ],
            if let Some(errors_section) = &errors_section {
                vec![errors_section
                    .to_html(NumberPath::empty(), &html)?
                    .to_html_string(&html)?]
            } else {
                vec![]
            },
            vec![
                format!("## Contents"),
                toc_html.to_html_string(&html)?,
                toplevel_section.to_markdown(NumberPath::empty(), &html)?,
            ],
            if opts.timestamp {
                vec![
                    format!("-------------------------------------------------------"),
                    format!("Last updated: {now}\n"),
                ]
            } else {
                vec![]
            },
        ])
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
        for file_errors in file_errorss {
            file_errors
                .print_plain(&mut out)
                .context("writing to stderr")?;
        }
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
                html.print_html(htmldocument, &mut out)?;
                out.flush()?;

                if opts.open_if_changed {
                    // Need to remember whether the file has changed
                    html_file_has_changed = !git(
                        &base_path,
                        &["diff", "--no-patch", "--exit-code", "--", HTML_FILENAME],
                    )?;
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

            // Commit files if requested and any were written
            if opts.commit && !written_files.is_empty() {
                // First check that there are no uncommitted changes
                let items = git_status(&base_path)?;
                let changed_items: Vec<_> = items
                    .iter()
                    .filter(|item| !written_files.contains(&item.path.as_str()))
                    .collect();
                if !changed_items.is_empty() {
                    bail!(
                        "won't carry out --commit request due to uncommitted \
                         changes in {base_path:?}: {changed_items:?}"
                    )
                }

                git(&base_path, &append(&["add", "-f", "--"], &written_files))?;

                let did_commit = git(
                    &base_path,
                    &append(
                        &[
                            "commit",
                            "-m",
                            &format!(
                                "regenerate index file{} via {PROGRAM_NAME}",
                                if written_files.len() > 1 { "s" } else { "" }
                            ),
                            "--",
                        ],
                        &written_files,
                    ),
                )?;

                if did_commit && opts.push {
                    git(&base_path, &["push"])?;
                }
            }
        } else {
            html.print_html(htmldocument, &mut stdout().lock())?;
        }
    }

    // Open a web browser if appropriate
    if opts.open || (opts.open_if_changed && html_file_has_changed) {
        if write_files {
            if let Some(base_path) = &opts.base_path {
                // Hopefully all browsers take relative paths?
                // Otherwise would have to resolve base_path with
                // HTML_FILENAME pushed-on as an absolute path:
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
