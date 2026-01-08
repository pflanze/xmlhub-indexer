// Use from the standard library
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet, HashSet},
    fs::{create_dir, File},
    io::{stderr, stdout, BufWriter, Write},
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
};

// Use from external dependencies
use ahtml::{att, flat::Flat, AId, HtmlAllocator, Node, Print, SerHtmlFrag};
use ahtml_from_markdown::markdown::markdown_to_html;
use anyhow::{anyhow, bail, Context, Result};
use chj_unix_util::{
    backoff::{LoopVerbosity, LoopWithBackoff},
    daemon::{Daemon, DaemonMode},
    file_lock::{file_lock_nonblocking, FileLockError},
    forking_loop::forking_loop,
};
use cj_path_util::path_util::AppendToPath;
use clap::Parser;
use itertools::Itertools;
use lazy_static::lazy_static;
use nix::sys::resource::{setrlimit, Resource};
use pluraless::pluralized;
use rayon::prelude::{IndexedParallelIterator, IntoParallelIterator, ParallelIterator};
use run_git::git::{BaseAndRelPath, GitStatusItem, GitWorkingDir};
use walkdir::WalkDir;

// Use from src/*.rs
use xmlhub_indexer::{
    beast_version::{check_beast_version, BeastProductVersion, BeastVersion},
    browser::{spawn_browser, spawn_browser_on_path},
    changelog::Changelog,
    checkout_context::{
        CheckExpectedSubpathsExist, CheckedCheckoutContext1, CheckedCheckoutContext2,
    },
    const_util::file_name,
    fixup_path::CURRENT_DIRECTORY,
    folder::Folder,
    get_terminal_width::get_terminal_width,
    git_version::{GitVersion, SemVersion},
    hints::Hints,
    html_util::anchor,
    installation::{
        binaries_repo::Os,
        defaults::global_app_state_dir,
        git_based_upgrade::{changelog_display, git_based_upgrade, UpgradeRules},
    },
    markdown_paragraphs,
    modified_xml_document::{ClearAction, ClearElementsOpts, ModifiedXMLDocument},
    rayon_util::ParRun,
    section::{Highlight, NumberPath, Section},
    string_tree::StringTree,
    tuple_transpose::TupleTranspose,
    util::format_string_list,
    util::{append, strip_prefixes, with_output_to_file, InsertValue},
    utillib::{
        file_util_with_trash::write_file_moving_to_trash_if_exists,
        setpriority::{possibly_setpriority, PriorityWhich},
    },
    version_info::VersionInfo,
    xml_document::{read_xml_file, XMLDocumentComment},
    xmlhub_attributes::{
        attribute_specification_by_name, sort_in_definition_order, AttributeName, AttributeNeed,
        AttributeSource, AttributeSpecification, KeyStringPreparation, METADATA_SPECIFICATION,
    },
    xmlhub_autolink::Autolink,
    xmlhub_check_version::XmlhubCheckVersion,
    xmlhub_clone_to::{clone_to_command, CloneToOpts},
    xmlhub_docs::{
        docs_command, help_attributes_command, help_contributing_command, make_attributes_md,
        HelpAttributesOpts, CONTRIBUTE_FILENAME,
    },
    xmlhub_file_issues::{FileErrors, FileIssues, FileWarnings},
    xmlhub_fileinfo::{
        AttributeValue, FileInfo, Issue, Metadata, WithCommentsOnly, WithDerivedValues,
        WithExtractedValues,
    },
    xmlhub_global_opts::{
        BlindingOpts, DrynessOpt, OpenOrPrintOpts, QuietOpt, VerbosityOpt, VersionCheckOpt,
    },
    xmlhub_help::print_basic_standalone_html_page,
    xmlhub_indexer_defaults::{
        css_styles, document_symbol, git_log_version_checker, BACK_TO_INDEX_SYMBOL,
        GENERATED_MESSAGE, HTML_ALLOCATOR_POOL, HTML_FILE, MD_FILE, PROGRAM_NAME,
        SEQUENCES_ELEMENT_NAME, SOURCE_CHECKOUT, XMLHUB_CHECKOUT,
    },
    xmlhub_install::{install_command, InstallOpts},
    xmlhub_types::OutputFile,
};

// -------------------------------------------------------------------------
// Various settings in addition to those imported from
// `xmlhub_indexer_defaults` (see `xmlhub_indexer_defaults.rs` to edit
// those!) and `src/xmlhub_global_opts.rs`.

/// Comment added to XML files when blinding `<data>` XML elements
const DEFAULT_COMMENT_FOR_BLINDED_DATA: &str =
    "Sequences removed due to terms of use or privacy concerns";

/// How many seconds to sleep at minimum between runs in daemon
/// mode. Keep in sync with the `Opts` docs above!
const MIN_SLEEP_SECONDS_DEFAULT: f64 = 10.;

/// Do not sleep more than that many seconds between runs.
const MAX_SLEEP_SECONDS: f64 = 1000.;

/// In daemon start mode with --quiet, log a single line every given
/// number of seconds (to give a signal about being alive). Note that
/// it will log less frequently if there were errors for a long time
/// and it is sleeping a long time due to backing off because of that.
const DAEMON_ACTIVITY_LOG_INTERVAL_SECONDS: u64 = 120;

/// Max size of a single log file in bytes before it is renamed.
const MAX_LOG_FILE_SIZE_DEFAULT: u64 = 1000000;

/// Max number of log files before they are deleted.
const MAX_LOG_FILES_DEFAULT: u32 = 100;

/// Address space memory limit set inside every worker child, in
/// bytes. Much is needed as the HtmlAllocator regions pre-allocate a
/// lot of virtual memory even if it is never needed. There are no RSS
/// resource limits in Linux (except via cgroups in some cases).
const AS_BYTES_LIMIT_IN_WORKER_CHILD: u64 = 3 * 1024 * 1024 * 1024;

/// Limit on CPU time, for the soft limit (a hard limit is set to 1
/// second higher than this value).
const CPU_SECONDS_LIMIT_IN_WORKER_CHILD: u64 = 5;

/// The file describing the attributes (for contributors).
const ATTRIBUTES_FILE: OutputFile = OutputFile {
    path_from_repo_top: "attributes.md",
};

const OUTPUT_FILES: [&OutputFile; 3] = [&HTML_FILE, &MD_FILE, &ATTRIBUTES_FILE];

// -------------------------------------------------------------------------
// Derived values:

/// The name of the program in the abstract (repository name).
const REPO_NAME: &str = file_name(SOURCE_CHECKOUT.supposed_upstream_web_url);

/// The value of this constant is generated by the `build.rs` program
/// during compilation. It is the output of running `git describe ..`
/// in `build.rs`.
const PROGRAM_VERSION: &str = env!("GIT_DESCRIBE");

lazy_static! {
    /// Name of the folder where the lock and log files are saved,
    /// placed at the root of the working directory.
    static ref DAEMON_FOLDER_NAME: String = format!(".{PROGRAM_NAME}");
}

// =============================================================================
// Specification of the command line interface, using the `clap`
// library crate.

#[derive(clap::Parser, Debug)]
#[clap(next_line_help = true)]
#[clap(set_term_width = get_terminal_width())]
/// A tool to work with XML Hub, a Git repository of BEAST2 files.
/// Start with the "docs" subcommand, it will tell you how to use
/// this program!
struct Opts {
    /// Show the program version (it was copied from `git describe
    /// --tags ..` at compile time) as well as some other information
    /// on the binary.
    // Note: can't name this field `version` as that's special-cased
    // in Clap.
    #[clap(long = "version")]
    v: bool,

    /// Like `--version` but show *only* the program version.
    #[clap(long)]
    version_only: bool,

    /// The subcommand to run. Use `--help` after the sub-command to
    /// get a list of the allowed options there.
    #[clap(subcommand)]
    command: Option<Command>,
}

#[derive(clap::Subcommand, Debug)]
enum Command {
    /// ** Start with this if you're contributing to XML Hub for the
    /// first time or have forgotten how things work! **
    Docs,
    /// Open the CONTRIBUTING documentation in the web browser.
    HelpContributing,
    /// Show all metadata attributes and describe their possible
    /// values. Use `--open` to open in browser.
    HelpAttributes(HelpAttributesOpts),
    /// Install this executable so that it can be run without having
    /// to specify the full path to it. Note: you have to start a new
    /// shell to pick up the change in the `PATH` environment variable
    /// setting.
    Install(InstallOpts),
    /// Upgrade this executable to the newest binary available from
    /// the `xmlhub-indexer-binaries` repository.
    Upgrade(UpgradeOpts),
    /// View the version history of this program
    Changelog(ChangelogOpts),
    /// Rebuild the XML Hub index, and by default commit the changed
    /// index. If you want to check your file while you edit it, use
    /// the `check` subcommand instead first.
    Build(BuildOpts),
    /// Check the correctness of a single file, without
    /// committing. Use this while editing. Once your document yields
    /// no more errors, run the `build` subcommand.
    Check(CheckOpts),
    /// Clone the XML Hub repository and apply merge config change.
    CloneTo(CloneToOpts),
    /// Prepare some XML file(s) by adding a metadata template to
    /// it/them so that the metadata can more easily be entered via a
    /// text editor, and by default, deleting sequence data.  Careful!: this
    /// replaces the files in place, but keeps the original in the
    /// system trash bin. Also see the `add` subcommand, which leaves
    /// the original file untouched but creates a prepared copy in a
    /// separate directory.
    Prepare(PrepareOpts),
    /// Add some XML file(s) to a XML Hub repository clone and carry
    /// out the `prepare` action on them at the same time. This leaves
    /// the original file unchanged. The next step afterwards is to
    /// edit the file and run the `check` subcommand until there are
    /// no errors.
    AddTo(AddToOpts),
}

#[derive(clap::Parser, Debug)]
struct UpgradeOpts {
    /// Even if the local executable is already up to date, re-install
    /// it anyway (rarely useful, except for re-running the
    /// installation of the shell settings, but `xmlhub install` will
    /// achieve the same?--TODO: add force option there).
    #[clap(long)]
    force_reinstall: bool,
    /// Install even if the local executable is newer than the
    /// downloaded one. Only use if you know what you're doing
    /// (normally your files and updated index files will not work
    /// problem-free for others!).
    #[clap(long)]
    force_downgrade: bool,
    /// Show what is going to be done and ask for confirmation
    #[clap(long)]
    confirm: bool,
}

#[derive(clap::Parser, Debug)]
struct ChangelogOpts {
    #[clap(flatten)]
    open_or_print: OpenOrPrintOpts,

    /// Which version to start from (exclusive)
    #[clap(long)]
    from: Option<GitVersion<SemVersion>>,
    /// Which version to end with (inclusive)
    #[clap(long)]
    to: Option<GitVersion<SemVersion>>,
    /// Whether it's OK to have `--from` > `--to`
    #[clap(long)]
    allow_downgrades: bool,
}

#[derive(clap::Parser, Debug)]
struct BuildOpts {
    #[clap(flatten)]
    dryness: DrynessOpt,
    #[clap(flatten)]
    verbosity: VerbosityOpt,
    #[clap(flatten)]
    versioncheck: VersionCheckOpt,
    #[clap(flatten)]
    quietness: QuietOpt,

    /// When running in `--daemon start` mode, for the log messages,
    /// use time stamps in the local time zone. The default is to use
    /// UTC.
    #[clap(long)]
    pub localtime: bool,

    /// When running in `--daemon start` mode, the maximum size of a
    /// log file in bytes before the current file is renamed and a new
    /// one is created instead. Default: 1000000.
    #[clap(long)]
    pub max_log_file_size: Option<u64>,

    /// When running in `--daemon start` mode, the number of numbered
    /// log files before the oldest files are automatically
    /// deleted. Careful: will delete as many files as needed to get
    /// their count down to the given number (if you give 0 it will
    /// delete them all.) Default: 100.
    #[clap(long)]
    pub max_log_files: Option<u32>,

    /// Write the index files (and commit them if requested) even if
    /// some files had errors and thus won't be indexed; the errors
    /// are written in a section at the top of the index files,
    /// though. The same errors are still printed to stderr, and
    /// reported as exit code 1, too, though--see
    /// `--ok-on-written-errors` to change that. Those errors are also
    /// committed (and pushed if `--push` was given), unless
    /// `--no-commit-errors` is also given.
    #[clap(long, short)]
    write_errors: bool,

    /// If you want to see errors in the browser and hence are using
    /// `--write-errors`, but don't want those to be committed (and
    /// pushed if `--push` was given), this option will prevent those
    /// latter steps (meaning errors are written to the files, but not
    /// committed).
    #[clap(long)]
    no_commit_errors: bool,

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

    /// Same as `--open` but only opens a browser if the index has
    /// changed since the last Git commit to it.
    #[clap(long)]
    open_if_changed: bool,

    /// Git pull from the default remote into the local Git checkout
    /// before creating the index files.
    #[clap(long)]
    pull: bool,

    /// Do not add and commit the output files to the Git
    /// repository. It's better to let `xmlhub` do that (the
    /// default) rather than doing it manually, since it adds its
    /// version information to the commit message and later
    /// invocations of it check whether it needs upgrading. So this
    /// option should only be used temporarily during development.
    #[clap(long)]
    no_commit: bool,

    /// Push the local Git changes to the default remote after
    /// committing. Does nothing if the `--no-commit` option was
    /// given, or if there were no changes.
    #[clap(long)]
    push: bool,

    /// Update the xmlhub repository unattended (e.g. via a cronjob or
    /// similar). Implies --write-errors, --silent-on-written-errors,
    /// --no-repo-check and --push, and disables --no-commit. Instead
    /// of --pull, uses `git remote update` and `git reset --hard` to
    /// set the local branch to the remote, which avoids the risk for
    /// merge conflicts but throws away local changes!  WARNING: this
    /// will lead to local changes being lost!  Do not use this option
    /// for interactive usage!
    #[clap(long)]
    batch: bool,

    /// Run as a daemon, i.e. do not exit, but run batch conversion
    /// repeatedly. The given string must be one of "run", "start",
    /// "start-if-not-running", "stop", "restart", "status". "run"
    /// does not put the process into the background, "start" (and
    /// "restart") does.  Implies `--batch`. You may want to use
    /// `--quiet` at the same time. Also see
    /// `--daemon-sleep-time`. When using "start" mode, writes logs to
    /// the directory `.xmlhub/logs/` under the given `BASE_PATH`. You
    /// probably want to also give `--quiet` to reduce the amount of
    /// log space required.
    #[clap(long)]
    daemon: Option<DaemonMode>,

    /// When running in one of the `--daemon` modes, use the given
    /// number of seconds as the minimum time to sleep between
    /// conversion runs; on errors this interval may be increased
    /// (exponential backoff). The default is 10 seconds.
    #[clap(long)]
    daemon_sleep_time: Option<f64>,

    /// Do not check that the correct branch is checked out in the
    /// xmlhub repository. Only use if you're experimenting on another
    /// branch.
    #[clap(long)]
    no_branch_check: bool,

    /// Omit the check for the Git clone at the `BASE_PATH` directory
    /// to contain items that make it look like a legit xmlhub
    /// repository clone.
    #[clap(long)]
    no_repo_check: bool,

    /// Ignore untracked files (local files not added to the xmlhub
    /// repository). By default, files are read regardless of whether
    /// they are in Git or not.
    #[clap(long)]
    ignore_untracked: bool,

    /// The path to the base directory of the Git checkout of the XML
    /// Hub. The default is `.`.
    #[clap(long)]
    base_path: Option<PathBuf>,

    /// The virtual address space limit for the child process carrying
    /// out a build when in daemon mode, in bytes (default: 3
    /// GiB). Only works on Linux, ignored on macOS as address space
    /// limiting is broken there.
    #[clap(long)]
    limit_as: Option<u64>,
}

#[derive(clap::Parser, Debug)]
struct CheckOpts {
    #[clap(flatten)]
    versioncheck: VersionCheckOpt,
    #[clap(flatten)]
    dryness: DrynessOpt,
    #[clap(flatten)]
    verbosity: VerbosityOpt,
    #[clap(flatten)]
    quietness: QuietOpt,

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

    /// Same as `--open` but only opens a browser if the index has
    /// changed since the last Git commit to it.
    #[clap(long)]
    open_if_changed: bool,

    /// Omit the check for the Git clone containing the FILE_PATHS to
    /// contain items that make it look like a legit xmlhub repository
    /// clone.
    #[clap(long)]
    no_repo_check: bool,

    /// The path(s) to the XML file(s) you're currently working on and
    /// want to check. Must be somewhere in a Git checkout of the XML
    /// Hub (this is because `check` will still rebuild the index, too
    /// (but never commit it), so that you can see the effect of your
    /// file).
    file_paths: Vec<PathBuf>,
}

#[derive(clap::Parser, Debug)]
struct PrepareOpts {
    #[clap(flatten)]
    quietness: QuietOpt,
    #[clap(flatten)]
    blinding: BlindingOpts,

    /// The path(s) to the XML file(s) which should be
    /// modified. Careful: they are modified in place (although the
    /// original is kept in the system trash bin)! Use the `add-to`
    /// subcommand instead if you really want to copy them. The
    /// metainformation template is added and sequence data is
    /// stripped (unless you provide the `--no-blind` option). .
    files_to_prepare: Vec<PathBuf>,

    /// Allow XML files from BEAST versions other than BEAST2. Note
    /// that you'll also have to provide `--no-blind` as blinding is
    /// only implemented for BEAST2.
    #[clap(long)]
    ignore_version: bool,
    // XX FUTURE idea: --set "header: value"
}

#[derive(clap::Parser, Debug)]
struct AddToOpts {
    #[clap(flatten)]
    versioncheck: VersionCheckOpt,
    #[clap(flatten)]
    quietness: QuietOpt,
    #[clap(flatten)]
    blinding: BlindingOpts,

    /// The path to an existing directory *inside* the Git checkout of
    /// the XML Hub, where the file(s) should be copied to. .
    target_directory: Option<PathBuf>,

    /// Omit the check for the `TARGET_PATH` directory to be in a Git
    /// clone that contains items that make it look like a legit
    /// xmlhub repository clone.
    #[clap(long)]
    no_repo_check: bool,

    /// The path(s) to the XML file(s), outside of the XML Hub
    /// repository, that you want to add to XML Hub. They are copied,
    /// and a metainformation template is added to them while doing
    /// so, and sequence data is stripped (unless you provide the
    /// `--no-blind` option). .
    files_to_add: Vec<PathBuf>,

    /// Create the `TARGET_DIRECTORY` if it doesn't exist yet. .
    #[clap(long)]
    mkdir: bool,

    /// Allow XML files from BEAST versions other than BEAST2. Note
    /// that you'll also have to provide `--no-blind` as blinding is
    /// only implemented for BEAST2.
    #[clap(long)]
    ignore_version: bool,

    /// Force overwriting of existing files at the target location. .
    #[clap(long, short)]
    force: bool,
    // XX FUTURE idea: --set "header: value"
}

// =============================================================================
// Parsing

/// Parse all XML comments from above the first XML opening element
/// out of one file as `Metadata`. The comments are passed as an
/// iterator over `XMLDocumentComment`, which has the string and
/// location of the comment. The `XMLDocumentComment` has a limited
/// lifetime (validity span) indicated by the context of the call to
/// `parse_comments`, hence passed as lifetime parameter `'a`. If
/// `dry` is true, does not parse the values; this is used in
/// `prepare_file` to check whether headers are complete without
/// checking the validity of the values.
fn parse_comments<'a>(
    comments: impl Iterator<Item = XMLDocumentComment<'a>>,
    dry: bool,
) -> Result<Metadata<WithCommentsOnly>, Vec<Issue>> {
    let spec_by_lowercase_key: BTreeMap<String, &AttributeSpecification> = METADATA_SPECIFICATION
        .iter()
        .map(|spec| (spec.key.as_ref().to_lowercase(), spec))
        .collect();
    let mut unseen_specs_by_lowercase_key = spec_by_lowercase_key.clone();
    let mut map: BTreeMap<AttributeName, AttributeValue> = BTreeMap::new();

    // Collect all errors instead of stopping at the first one.
    let mut errors: Vec<Issue> = Vec::new();
    for comment in comments {
        // Using a function without arguments and calling it right
        // away to capture the result (Ok or Err).
        let result = (|| {
            if let Some((key_, value)) = comment.string.split_once(":") {
                let lc_key = key_.trim().to_lowercase();
                let value = value.trim();

                if let Some(spec) = spec_by_lowercase_key.get(&lc_key) {
                    unseen_specs_by_lowercase_key.remove(&lc_key);
                    if map.contains_key(&spec.key) {
                        bail!("duplicate entry for attribute name {lc_key:?}")
                    } else {
                        if !dry {
                            let value = AttributeValue::from_str_and_spec(value, spec)?;
                            map.insert(spec.key, value);
                        }
                    }
                } else {
                    bail!("unknown attribute name {lc_key:?} given")
                }
            } else {
                bail!("comment does not start with a keyword name and ':'")
            }
            Ok(())
        })()
        .with_context(|| anyhow!("XML comment on {}", comment.location));
        if let Err(e) = result {
            errors.push(Issue {
                message: format!("{e:#}"),
                hint: None,
            });
        }
    }

    let missing: Vec<AttributeName> = unseen_specs_by_lowercase_key
        .into_values()
        .filter_map(|spec| {
            let source_spec = match &spec.source {
                AttributeSource::Specified(source_spec) => source_spec,
                AttributeSource::Derived(_) | AttributeSource::Extracted(_) => return None,
            };
            // Do not report as missing if it's optional
            if source_spec.need == AttributeNeed::Optional {
                None
            } else {
                Some(spec.key)
            }
        })
        .collect();
    if !missing.is_empty() {
        let sorted_missing: Vec<AttributeName> =
            sort_in_definition_order(missing.into_iter().map(|k| (k, ())))
                .into_iter()
                .filter_map(|(k, v)| {
                    v?;
                    Some(k)
                })
                .collect();

        pluralized! { sorted_missing.len() => attributes, these, names, are }
        errors.push(Issue {
            message: format!(
                "{attributes} with {these} {names} {are} missing: {}",
                // Show just the names, not the AttributeName wrappers
                format_string_list(&sorted_missing),
            ),
            hint: None,
        });
    }

    if errors.is_empty() {
        Ok(Metadata::new(map))
    } else {
        Err(errors)
    }
}

lazy_static! {
    static ref VERSION_KEY: AttributeName = attribute_specification_by_name("Version")
        .map(|spec| spec.key)
        .expect("'Version' attribute definition should always be present");
}

/// Map each file to the info extracted from it (or `FileErrors`
/// when there were errors), including path and an id, held in a
/// `FileInfo` struct. Generate the ids on the go for each of them
/// by `enumerate`ing the values (the enumeration number value is
/// passed as the `id` argument to the function given to `map`).
/// The id is used to refer to each item in document-local links in
/// the generated HTML/Markdown files.
fn read_file_infos(
    paths: Vec<BaseAndRelPath>,
) -> Vec<Result<FileInfo<WithExtractedValues>, FileErrors>> {
    paths
        .into_par_iter()
        .enumerate()
        .map(
            |(id, path)| -> Result<FileInfo<WithExtractedValues>, FileErrors> {
                let xmldocument = read_xml_file(&path.full_path()).map_err(|e| FileErrors {
                    path: path.clone(),
                    errors: vec![Issue {
                        message: format!("{e:#}"),
                        hint: None,
                    }],
                })?;
                let metadata =
                    parse_comments(xmldocument.header_comments(), false).map_err(|errors| {
                        FileErrors {
                            path: path.clone(),
                            errors,
                        }
                    })?;

                let mut warnings: Vec<Issue> = Vec::new();

                let metadata = metadata.add_extracted_attributes(&xmldocument, &mut warnings);

                // Check the version in the XML: verify that it fits
                // what the user provided in the XML comment.
                match (|| -> Result<_> {
                    let att_val: &AttributeValue = metadata
                        .get(*VERSION_KEY)
                        .context("missing 'Version' entry")?;
                    let att_vals = att_val.as_string_list();
                    let user_specified_str =
                        att_vals.first().context("'Version' entry is empty")?;
                    // Have to skip any "BEAST "
                    let user_specified_version_str = strip_prefixes(
                        user_specified_str,
                        &["BEAST2 ", "BEAST2", "BEAST ", "BEAST"],
                    )
                    .trim();
                    let user_specified_version =
                        BeastVersion::from_str(user_specified_version_str)?;
                    let user_specified_major: u16 = user_specified_version.major.context(
                        "provided 'Version' has no BEAST2-major number part \
                         or is not a BEAST2 version",
                    )?;

                    let document_version =
                        check_beast_version(xmldocument.document(), path.rel_path(), false)?;
                    (|| -> Option<()> {
                        let found_major: u16 = document_version.major?;
                        if found_major != user_specified_major {
                            warnings.push(Issue {
                                message: format!(
                                    "the <beast> element in the document specifies version \
                                     {document_version} with major {found_major}, but the \
                                     user-provided version {user_specified_version} has \
                                     major {user_specified_major}"
                                ),
                                hint: Some(
                                    "Please edit the file to make both versions match \
                                     the BEAST version you're actually using."
                                        .into(),
                                ),
                            });
                        }
                        Some(())
                    })();
                    Ok(())
                })() {
                    Ok(()) => (),
                    // XX why a warning for an error?
                    Err(e) => warnings.push(Issue {
                        message: format!("{e}"),
                        hint: None,
                    }),
                }

                Ok(FileInfo {
                    id,
                    path,
                    metadata,
                    warnings,
                })
            },
        )
        .collect()
}

// =============================================================================
// Building output / implementing the various subcommands

/// Build an index, as human-readable text (thus as `Section`), over
/// all files for one particular attribute name (`attribute_key`).
fn build_index_section(
    attribute_key: AttributeName,
    key_string_normalization: KeyStringPreparation,
    autolink: Autolink,
    file_infos: &[FileInfo<WithDerivedValues>],
) -> Result<Section> {
    // Build an index by the value for attribute_key (lower-casing the
    // key values for consistency if use_lowercase is true). The index
    // maps from key value to a set of all `FileInfo`s for that
    // value. The BTreeMap keeps the key values sorted alphabetically,
    // which is nice so we don't have to sort those afterwards.
    let mut file_infos_by_key_string: BTreeMap<String, BTreeSet<&FileInfo<WithDerivedValues>>> =
        BTreeMap::new();

    for file_info in file_infos {
        if let Some(attribute_value) = file_info.metadata.get(attribute_key) {
            for key_string in attribute_value.as_string_list().iter() {
                file_infos_by_key_string.insert_value(
                    key_string_normalization.prepare_key_string(key_string),
                    file_info,
                );
            }
        }
    }

    let html = HTML_ALLOCATOR_POOL.get();

    // The contents of the section, i.e. the list of all key_strings and
    // the files for the respective key_string.
    let mut body = html.new_vec();
    for (key_string, file_infos) in &file_infos_by_key_string {
        // Output the key value, with an anchor
        let anchor_name = attribute_key.anchor_name(key_string);
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
                html.i(
                    [],
                    html.q(
                        [],
                        anchor(
                            &anchor_name,
                            autolink.format_html(key_string, &*html)?,
                            &html,
                        )?,
                    )?,
                )?,
            )?,
        )?)?;

        // Output all the files for that key value, sorted by path.
        let mut sorted_file_infos: Vec<&FileInfo<WithDerivedValues>> =
            file_infos.iter().copied().collect();
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
                        html.text(rel_path)?,
                    )?,
                    html.nbsp()?,
                    html.a(
                        [att("href", rel_path), att("title", "Open the file")],
                        document_symbol(&html)?,
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
        highlight: Highlight::None,
        title: Some(attribute_key.as_ref().into()),
        intro: Some(html.preserialize(html.dl([att("class", "key_dl")], body)?)?),
        subsections: vec![],
    })
}

/// Create a `<div>&nbsp;<br>...</div>` occupying some amount of
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

/// Make an intro, slightly differently depending on whether it is
/// for the .md or .html file.
fn make_intro(making_md: bool, html: &HtmlAllocator) -> Result<AId<Node>> {
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
                        [att("href", format!("{CONTRIBUTE_FILENAME}.md"))],
                        html.text(CONTRIBUTE_FILENAME)?,
                    )?,
                    html.text(".")?,
                ],
            )?,
            html.p(
                [],
                [
                    html.text("This is an index over all XML files, generated by ")?,
                    html.a(
                        [att("href", SOURCE_CHECKOUT.supposed_upstream_web_url)],
                        html.text(REPO_NAME)?,
                    )?,
                    html.text(".")?,
                ],
            )?,
            html.p(
                [],
                [
                    html.text(
                        "From the index, click on a link to jump to the info box \
                         about that file, or on the ",
                    )?,
                    document_symbol(html)?,
                    html.text(format!(
                        " symbol to open the XML file directly. From the info box, \
                         click on the {BACK_TO_INDEX_SYMBOL} symbol to jump to the \
                         index position for that value.",
                    ))?,
                ],
            )?,
            html.p(
                [],
                [html.text(
                    "You can also search the contents of all files via the \
                     GitLab search form, which you can find towards the top left \
                     corner of this page (the input field saying \"Search or go \
                     to...\").",
                )?],
            )?,
            if making_md {
                html.p(
                    [],
                    html.small(
                        [],
                        html.text(format!(
                            "Note: if you \"git clone\" this repository, open the file \
                             {:?} instead, it has the same info already \
                             formatted as HTML (and in fact has better formatting than \
                             the view you're seeing here).",
                            HTML_FILE.path_from_repo_top
                        ))?,
                    )?,
                )?
            } else {
                html.empty_node()?
            },
        ],
    )
}

/// The subset of the options of `BuildOpts` used by `build_index`
struct BuildIndexOpts {
    dryness: DrynessOpt,
    verbosity: VerbosityOpt,
    quietness: QuietOpt,

    pull: bool,
    batch: bool,
    ignore_untracked: bool,
    write_errors: bool,
    silent_on_written_errors: bool,
    ok_on_written_errors: bool,
    open_if_changed: bool,
    no_commit: bool,
    no_commit_errors: bool,
    no_branch_check: bool,
    open: bool,
}

/// Run one conversion from the XML files to the index files. Returns
/// the exit code to exit the program with.
fn build_index(
    build_index_opts: BuildIndexOpts,
    git_log_version_checker: &XmlhubCheckVersion,
    xmlhub_checkout: &CheckedCheckoutContext1<Cow<Path>>,
    maybe_checked_xmlhub_checkout: &Option<CheckedCheckoutContext2<Cow<Path>>>,
) -> Result<i32> {
    let BuildIndexOpts {
        dryness: DrynessOpt { dry_run },
        verbosity: VerbosityOpt { verbose },
        quietness,
        pull,
        batch,
        ignore_untracked,
        write_errors,
        silent_on_written_errors,
        ok_on_written_errors,
        open_if_changed,
        no_commit,
        no_commit_errors,
        no_branch_check,
        open,
    } = build_index_opts;

    // Define a macro to only run $body if opts.dry_run is false,
    // otherwise show $message instead, or show $message anyway if
    // opts.verbose.
    macro_rules! check_dry_run {
        { message: $message:expr, $body:expr } => {
            let s = || -> String { $message.into() };
            if dry_run {
                xmlhub_indexer::dry_run::eprintln_dry_run(s());
            } else {
                if verbose {
                    xmlhub_indexer::dry_run::eprintln_running(s());
                }
                $body;
            }
        }
    }

    // Update repository if requested
    if let Some(checked_xmlhub_checkout) = maybe_checked_xmlhub_checkout {
        if pull {
            check_dry_run! {
                message: "git pull",
                if !xmlhub_checkout.git_working_dir().git( &["pull"],
                        quietness.quiet())? {
                    bail!("git pull failed")
                }
            }
        }

        if batch {
            let default_remote = &checked_xmlhub_checkout.default_remote;

            check_dry_run! {
                message: format!("git remote update {default_remote:?}"),
                if !xmlhub_checkout.git_working_dir().git(

                    &["remote", "update", default_remote],
                    quietness.quiet()
                )? {
                    bail!("git remote update {default_remote:?} failed")
                }
            }

            let remote_banch_reference = checked_xmlhub_checkout.remote_branch_reference();

            check_dry_run! {
                message: format!("git reset --hard {remote_banch_reference:?}"),
                if !xmlhub_checkout.git_working_dir().git(

                    &["reset", "--hard", &remote_banch_reference],
                    quietness.quiet()
                )? {
                    bail!("git reset --hard {remote_banch_reference:?} failed")
                }
            }
        }
    }

    // Get the list of files in the Git repo given by the base_path
    // option. Collect them as a vector of `RelPathWithBase` values,
    // each of which carries both a path to a base directory
    // (optional) and a relative path from there (if it contains no
    // base directory, the current working directoy is the base).
    let paths: Vec<BaseAndRelPath> = {
        git_log_version_checker.check_git_log()?;

        // Get the paths from running `git ls-files` inside the
        // directory at base_path, then ignore all files that don't
        // end in .xml
        let mut paths = if ignore_untracked {
            // Ask Git for the list of files
            xmlhub_checkout.git_working_dir().git_ls_files()?
        } else {
            // Ask the filesystem for the list of files, but do not
            // waste time listing paths in the .git nor .xmlhub
            // subdirs
            let ignored_file_names = HashSet::from([".git", &*DAEMON_FOLDER_NAME]);
            let entries = WalkDir::new(xmlhub_checkout.working_dir_path())
                .follow_links(false)
                .min_depth(1)
                .into_iter()
                .filter_entry(|entry| {
                    if let Some(file_name) = entry.file_name().to_str() {
                        !ignored_file_names.contains(file_name)
                    } else {
                        // invalid encoding; XX: what to do? Try to keep those:
                        true
                    }
                });
            let shared_base_path = Arc::new(xmlhub_checkout.working_dir_path().to_owned());
            let mut paths: Vec<BaseAndRelPath> = Vec::new();
            for entry in entries {
                let entry = entry.with_context(|| {
                    anyhow!(
                        "listing contents of directory {:?}",
                        xmlhub_checkout.working_dir_path()
                    )
                })?;
                let relative_path = entry
                    .path()
                    .strip_prefix(xmlhub_checkout.working_dir_path())
                    .with_context(|| {
                        // Could happen via folder rename races, right? So don't panic.
                        anyhow!(
                            "listed files of directory {:?} \
                             should be prefixed with that path, but got {:?}",
                            xmlhub_checkout.working_dir_path(),
                            entry.path()
                        )
                    })?;
                paths.push(BaseAndRelPath::new(
                    Some(Arc::clone(&shared_base_path)),
                    relative_path.to_owned(),
                ));
            }
            paths
        };
        paths.retain(|path| {
            if let Some(ext) = path.extension() {
                ext.eq_ignore_ascii_case("xml")
            } else {
                false
            }
        });
        // Sort entries ourselves out of a worry that git ls-files
        // might not guarantee a sort order. (The sort order
        // determines the ID assignment that happens later, and those
        // are used in the HTML output, hence would lead to useless
        // commits.)
        paths.sort_by(|a, b| a.rel_path().cmp(b.rel_path()));
        // Move `paths` to the variable with the same name in the
        // outer scope.
        paths
    };

    // See help text on `read_file_infos` for what it's doing.
    let fileinfo_or_errors: Vec<Result<FileInfo<WithExtractedValues>, FileErrors>> =
        read_file_infos(paths);

    // Partition fileinfo_or_errors into vectors with only the
    // successful and only the erroneous results.
    let (file_infos, file_errorss): (Vec<FileInfo<WithExtractedValues>>, Vec<FileErrors>) =
        fileinfo_or_errors.into_iter().partition_result();

    // Build derived attribute values. Errors during this phase are
    // stored as warnings, so as to not prevent users from pushing
    // their changes, since some errors could be temporary.
    let file_infos: Vec<FileInfo<WithDerivedValues>> = file_infos
        .into_iter()
        .map(|info| {
            let FileInfo {
                id,
                path,
                metadata,
                mut warnings,
            } = info;
            let metadata = metadata.add_derived_attributes(&mut warnings);
            FileInfo {
                id,
                path,
                metadata,
                warnings,
            }
        })
        .collect();

    let warningss: Vec<FileWarnings> = file_infos
        .iter()
        .filter_map(|info| info.opt_warnings())
        .collect();

    // Build the HTML fragments to use in the HTML page and the Markdown
    // file.

    // Create all the sections making up the output file(s)

    // Calculate the sections in parallel (first make a tuple with
    // argument-less anonymous functions, then call `par_run` on it
    // which evaluates each function potentially in parallel and
    // returns a tuple with the results, which we then call
    // `transpose` on to move error values up so they (well, the first
    // one found) can easily be propagated via `?`)
    let (file_info_boxes_section, index_sections_section, errors_section, warnings_section) = (
        // Create a Section with boxes with the metainfo for all XML
        // files, in a hierarchy reflecting the folder hierarchy where
        // they are.
        || -> Result<Section> {
            // Temporarily create a folder hierarchy from all the paths,
            // then convert it to a Section.

            let mut folder = Folder::new();
            for file_info in &file_infos {
                folder.add(file_info).expect("no duplicates");
            }
            // This being the last expression in a { } block returns
            // (moves) its value to the `file_info_boxes_section`
            // variable outside.
            folder.to_section(Some("File info by folder".into()))
        },
        // Create all indices for those metadata entries for which their
        // specification says to index them. Each index is in a separate
        // `Section`, but all are bundled as subsections in a single `Section`.
        || -> Result<Section> {
            let index_sections: Vec<Section> = METADATA_SPECIFICATION
                .into_par_iter()
                .filter_map(|spec| {
                    // Get a `KeyStringPreparation` instance if
                    // indexing is desired, if we got one we build an
                    // index; if we got none, `map` also returns
                    // `None`, which is dropped by `filter_map`.
                    spec.indexing
                        .key_string_preparation()
                        .map(|prep| build_index_section(spec.key, prep, spec.autolink, &file_infos))
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(Section {
                highlight: Highlight::None,
                title: Some("Index by attribute".into()),
                intro: None,
                subsections: index_sections,
            })
        },
        // Make an optional `Section` with all the errors if there are any
        || -> Result<Option<Section>> {
            if file_errorss.is_empty() {
                Ok(None)
            } else {
                let html = HTML_ALLOCATOR_POOL.get();
                let mut hints = Hints::new("errors");
                let mut items = html.new_vec();
                for file_errors in &file_errorss {
                    items.push_flat(file_errors.to_html(
                        true, // XX where is this defined?
                        "box", &mut hints, &html,
                    )?)?;
                }
                let intro_html = html.div([], [html.dl([], items)?, hints.to_html(&html)?])?;

                Ok(Some(Section {
                    highlight: Highlight::Red,
                    title: Some("Errors".into()),
                    intro: Some(html.preserialize(intro_html)?),
                    subsections: vec![],
                }))
            }
        },
        // Make an optional `Section` with all the warnings if there
        // are any -- COPYPASTE from above except for input value,
        // color, and title.
        || -> Result<Option<Section>> {
            if warningss.is_empty() {
                Ok(None)
            } else {
                let html = HTML_ALLOCATOR_POOL.get();
                let mut hints = Hints::new("warnings");
                let mut items = html.new_vec();
                for warnings in &warningss {
                    items.push_flat(warnings.to_html(
                        true, // XX where is this defined?
                        "box", &mut hints, &html,
                    )?)?;
                }
                let intro_html = html.div([], [html.dl([], items)?, hints.to_html(&html)?])?;

                Ok(Some(Section {
                    highlight: Highlight::Orange,
                    title: Some("Warnings".into()),
                    intro: Some(html.preserialize(intro_html)?),
                    subsections: vec![],
                }))
            }
        },
    )
        .par_run()
        .transpose()?;

    // Create a single section without a title, to enclose all the
    // other sections. This way, creating the table of contents and
    // conversion to HTML vs. Markdown works seamlessly.
    let toplevel_section = Section {
        highlight: Highlight::None,
        title: None,
        intro: None,
        subsections: append(
            append(
                // This converts the optional `errors_section` from an
                // Option<Section> to a Vec<Section> that contains 0 or 1
                // sections.
                errors_section.into_iter().collect::<Vec<_>>(),
                // Same again,
                warnings_section.into_iter().collect::<Vec<_>>(),
            ),
            // Always use the file_info_boxes_section and the index
            // sections.
            vec![index_sections_section, file_info_boxes_section],
        ),
    };

    let html = HTML_ALLOCATOR_POOL.get();

    // Some variables used in both the .html and .md documents
    let title = "XML Hub file index";
    let toc_html: SerHtmlFrag =
        html.preserialize(toplevel_section.to_toc_html(NumberPath::empty(), &html)?)?;

    // (For an explanation of the HTML creation syntax used below, see
    // the comment "The first list passed" further above.)

    // The contents for the README.html document
    let make_htmldocument = |html: &HtmlAllocator| -> Result<AId<Node>> {
        html.html(
            [],
            [
                html.head(
                    [],
                    [
                        html.meta(
                            [
                                att("name", "generator"),
                                att("content", &*GENERATED_MESSAGE),
                            ],
                            [],
                        )?,
                        html.meta(
                            [att("name", "author"), att("content", &*GENERATED_MESSAGE)],
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
                        make_intro(false, html)?,
                        html.h2([], html.text("Contents")?)?,
                        html.preserialized(toc_html.clone())?,
                        html.div([], toplevel_section.to_html(NumberPath::empty(), html)?)?,
                        empty_space_element(40, html)?,
                    ],
                )?,
            ],
        )
    };

    // The contents for the README.md document
    let make_mddocument = || -> Result<StringTree> {
        let html = HTML_ALLOCATOR_POOL.get();

        Ok(markdown_paragraphs![
            format!(
                "<!-- NOTE: {}, do not edit manually! -->",
                *GENERATED_MESSAGE
            ),
            format!("# {title}"),
            make_intro(true, &html)?.to_html_fragment_string(&html)?,
            "## Contents",
            toc_html.as_arc_str(),
            toplevel_section.to_markdown(NumberPath::empty())?,
            empty_space_element(40, &html)?.to_html_fragment_string(&html)?,
        ])
    };

    let have_errors = !file_errorss.is_empty();
    let have_warnings = !warningss.is_empty();

    // The behaviour of the program in the face of errors depends on 3
    // command line options. Here's the logic that derives 3
    // behaviours from the 3 options (it's not 1:1). The Rust compiler
    // verifies that each of the 3 variables is set exactly once.
    let exit_code;
    let write_errors_to_stderr;
    let write_files;
    if !have_errors {
        exit_code = 0;
        write_errors_to_stderr = false;
        write_files = true;
    } else {
        if write_errors {
            write_files = true;
            if silent_on_written_errors {
                exit_code = 0;
                write_errors_to_stderr = false;
            } else {
                write_errors_to_stderr = true;
                if ok_on_written_errors {
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

    let write_warnings_to_stderr;
    if !have_warnings {
        write_warnings_to_stderr = false;
    } else {
        if write_errors {
            if silent_on_written_errors {
                write_warnings_to_stderr = false;
            } else {
                write_warnings_to_stderr = true;
            }
        } else {
            write_warnings_to_stderr = true
        }
    }

    if write_errors_to_stderr {
        let mut out = stderr().lock();
        (|| -> Result<()> {
            writeln!(&mut out, "\nIndexing errors:")?;
            let mut hints = Hints::new("indexingerrors");
            for file_errors in file_errorss {
                file_errors.print_plain(&mut hints, &mut out)?
            }
            hints.print_plain(&mut out)?;
            Ok(())
        })()
        .context("writing to stderr")?;
    }

    if write_warnings_to_stderr {
        let mut out = stderr().lock();
        (|| -> Result<()> {
            writeln!(&mut out, "\nIndexing warnings:\n")?;
            let mut hints = Hints::new("indexingwarnings");
            for warning in warningss {
                warning.print_plain(&mut hints, &mut out)?
            }
            writeln!(&mut out, "")?;
            hints.print_plain(&mut out)?;
            Ok(())
        })()
        .context("writing to stderr")?;
    }

    let html_file_has_changed;
    if write_files {
        (html_file_has_changed, (), ()) = (
            || -> Result<_> {
                let html = HTML_ALLOCATOR_POOL.get();

                // Get an owned version of the base path and then
                // append path segments to it.
                let mut path = xmlhub_checkout.working_dir_path().to_owned();
                path.push(HTML_FILE.path_from_repo_top);
                let mut out = BufWriter::new(File::create(&path)?);
                html.print_html_document(make_htmldocument(&html)?, &mut out)?;
                out.flush()?;

                let mut html_file_has_changed = false;
                if open_if_changed {
                    // Need to remember whether the file has changed
                    check_dry_run! {
                        message: "git diff",
                        html_file_has_changed = !xmlhub_checkout.git_working_dir().git(

                            &["diff", "--no-patch", "--exit-code", "--",
                              HTML_FILE.path_from_repo_top],
                            false
                        )?
                    }
                }
                Ok(html_file_has_changed)
            },
            || -> Result<_> {
                let mut path = xmlhub_checkout.working_dir_path().to_owned();
                path.push(MD_FILE.path_from_repo_top);
                make_mddocument()?
                    .write_to_file(&path)
                    .with_context(|| anyhow!("writing to file {path:?}"))?;
                Ok(())
            },
            || -> Result<_> {
                let mut path = xmlhub_checkout.working_dir_path().to_owned();
                path.push(ATTRIBUTES_FILE.path_from_repo_top);
                make_attributes_md(true)?
                    .write_to_file(&path)
                    .with_context(|| anyhow!("writing to file {path:?}"))?;
                Ok(())
            },
        )
            .par_run()
            .transpose()?;

        let written_files = OUTPUT_FILES.map(|o| o.path_from_repo_top);

        // Commit files if not prevented by --no-commit, and any
        // were written, and --no-commit-errors was not given or
        // there were no errors. I.e. reasons not to commit:
        let no_commit_files =
            no_commit || written_files.is_empty() || (have_errors && no_commit_errors);
        let do_commit_files = !no_commit_files;
        if do_commit_files {
            if !no_branch_check {
                // Are we on the expected branch? NOTE: unlike most
                // checks on the repository, this one occurs late, but
                // we can't move it earlier if we want it to be
                // conditional on the need to actually commit.
                xmlhub_checkout.check_current_branch()?;
            }

            // Check that there are no uncommitted changes
            let mut items: Vec<GitStatusItem> = vec![];
            check_dry_run! {
                message: "git status",
                items = xmlhub_checkout.git_working_dir().git_status()?
            }
            let daemon_folder_name_with_slash = format!("{}/", *DAEMON_FOLDER_NAME);
            let ignore_path = |path: &str| -> bool {
                written_files.contains(&path) || path == daemon_folder_name_with_slash
            };
            let changed_items: Vec<String> = items
                .iter()
                .filter(|item| {
                    // Ignore untracked files in batch mode (they will
                    // be killed by reset --hard if in the way, and if
                    // not, then they could not have been created (XX
                    // unless there is a bug in the app, though,
                    // actually))
                    !(ignore_path(item.path.as_str()) || (batch && item.is_untracked(false)))
                })
                .map(|item| item.to_string())
                .collect();
            if !changed_items.is_empty() {
                // Avoid making this message look like a failure?
                // Hence do not use `bail!`, but just `eprintln!` with
                // a multi-line message, and return an error exit code
                // explicitly.
                eprintln!(
                    "\nFinished build, but won't run git commit due to uncommitted changes in {:?}:\n\
                     {}{}\n\
                     (Note: use the --no-commit option to suppress this error.)",
                    xmlhub_checkout.working_dir_path,
                    "  ",
                    changed_items.join("\n  "),
                );
                return Ok(1);
            }

            check_dry_run! {
                message: format!("git add -f -- {written_files:?}"),
                xmlhub_checkout.git_working_dir().git(

                    &append(&["add", "-f", "--"], &written_files),
                    quietness.quiet()
                )?
            }

            let mut did_commit = true;
            check_dry_run! {
                message: format!("git commit -m .. -- {written_files:?}"),
                did_commit = xmlhub_checkout.git_working_dir().git(

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
                    quietness.quiet()
                )?
            }

            if let Some(checked_xmlhub_checkout) = maybe_checked_xmlhub_checkout {
                let default_remote_for_push = &checked_xmlhub_checkout.default_remote;
                if did_commit {
                    check_dry_run! {
                        message: format!("git push {default_remote_for_push:?}"),
                        xmlhub_checkout.git_working_dir().git_push::<&str>(

                            default_remote_for_push,
                            &[],
                            quietness.quiet()
                        )?
                    }
                } else {
                    if !quietness.quiet() {
                        println!("There were no changes to commit, thus not pushing.")
                    }
                }
            }
        }
    } else {
        html_file_has_changed = false;
    }

    // Open a web browser if appropriate
    if open || (open_if_changed && html_file_has_changed) {
        if write_files {
            // Hopefully all browsers take relative paths? Firefox
            // on Linux and macOS are OK, Safari (via open -a) as
            // well.  Otherwise would have to resolve base_path
            // with HTML_FILENAME pushed-on as an absolute path:
            // let mut path = base_path.clone();
            // path.push(HTML_FILENAME);
            // path.canonicalize().as_os_str()
            spawn_browser(
                xmlhub_checkout.working_dir_path(),
                &[HTML_FILE.path_from_repo_top.as_ref()],
            )?;
        } else {
            eprintln!(
                "Note: not opening browser because the files weren't written due \
                 to errors; specify --write-errors if you want to write and open \
                 the file with the errors"
            );
        }
    }

    Ok(exit_code)
}

fn typed_from_no_repo_check(no_repo_check: bool) -> CheckExpectedSubpathsExist {
    if no_repo_check {
        CheckExpectedSubpathsExist::No
    } else {
        CheckExpectedSubpathsExist::Yes
    }
}

/// Execute an `upgrade` command
fn upgrade_command(
    program_version: GitVersion<SemVersion>,
    command_opts: UpgradeOpts,
) -> Result<()> {
    let UpgradeOpts {
        force_reinstall,
        force_downgrade,
        confirm,
    } = command_opts;

    git_based_upgrade(
        UpgradeRules {
            current_version: program_version,
            force_downgrade,
            force_reinstall,
            confirm,
        },
        &global_app_state_dir()?.upgrades_log_base()?,
    )?;

    Ok(())
}

/// Execute a `changelog` command
fn changelog_command(command_opts: ChangelogOpts) -> Result<()> {
    let ChangelogOpts {
        from,
        to,
        allow_downgrades,
        open_or_print,
    } = command_opts;

    let changelog = Changelog::new_builtin()?;
    let part =
        changelog.get_between_versions(allow_downgrades, false, from.as_ref(), to.as_ref())?;

    let print_markdown_to = |out: &mut dyn Write| write!(out, "{}", changelog_display(&part));

    let print_html_to = |output: &mut dyn Write| -> Result<()> {
        let mut out = Vec::new();
        print_markdown_to(&mut out)?;
        let markdown = String::from_utf8(out)?;
        let html = HTML_ALLOCATOR_POOL.get();
        let processed_markdown = markdown_to_html(&markdown, &html)?;
        print_basic_standalone_html_page(
            "xmlhub changelog",
            Flat::One(processed_markdown.html()),
            &html,
            output,
        )?;
        Ok(())
    };

    if open_or_print.do_open() {
        let base = global_app_state_dir()?.docs_base(PROGRAM_VERSION)?;
        let filename = format!("{}.html", part.display_title(false).0);
        let output_path = base.append(filename);
        with_output_to_file(&output_path, |output| -> Result<()> {
            Ok(print_html_to(output)?)
        })?;
        spawn_browser_on_path(&output_path)?;
    }

    if open_or_print.do_print() {
        let mut output = BufWriter::new(stdout().lock());
        print_markdown_to(&mut output)?;
        output.flush()?;
    }

    Ok(())
}

/// Execute a `build` command: prepare and run `build_index` in the
/// requested mode (interactive, batch, daemon). (Never returns `Ok`
/// but exits directly in the non-`Err` case. `!` is not stable yet.)
fn build_command(program_version: GitVersion<SemVersion>, build_opts: BuildOpts) -> Result<()> {
    let BuildOpts {
        dryness,
        verbosity,
        versioncheck: VersionCheckOpt { no_version_check },
        quietness,
        write_errors,
        no_commit_errors,
        ok_on_written_errors,
        silent_on_written_errors,
        open,
        open_if_changed,
        pull,
        no_commit,
        push,
        batch,
        daemon,
        daemon_sleep_time,
        no_branch_check,
        no_repo_check,
        ignore_untracked,
        base_path,
        localtime,
        max_log_file_size,
        max_log_files,
        limit_as,
    } = build_opts;

    let no_repo_check = typed_from_no_repo_check(no_repo_check);

    let xmlhub_checkout: CheckedCheckoutContext1<Cow<Path>> = if let Some(base_path) = base_path {
        XMLHUB_CHECKOUT
            .replace_working_dir_path(base_path.into())
            .check1(no_repo_check)?
    } else {
        XMLHUB_CHECKOUT.checked_from_subpath(*CURRENT_DIRECTORY, no_repo_check, false)?
    };

    // For pushing, need the `CheckedCheckoutContext` (which has the
    // `default_remote`). Retrieve this early to avoid committing and
    // then erroring out on pushing
    let maybe_checked_xmlhub_checkout = if push {
        Some(xmlhub_checkout.clone().check2()?)
    } else {
        None
    };

    let git_log_version_checker = git_log_version_checker(
        program_version,
        no_version_check,
        xmlhub_checkout.git_working_dir().into(),
    );

    let min_sleep_seconds = daemon_sleep_time.unwrap_or(MIN_SLEEP_SECONDS_DEFAULT);

    let build_index_once = || {
        build_index(
            BuildIndexOpts {
                dryness: dryness.clone(),
                verbosity: verbosity.clone(),
                quietness: quietness.clone(),
                pull,
                batch,
                ignore_untracked,
                write_errors,
                silent_on_written_errors,
                ok_on_written_errors,
                open_if_changed,
                no_commit,
                no_commit_errors,
                no_branch_check,
                open,
            },
            &git_log_version_checker,
            &xmlhub_checkout,
            &maybe_checked_xmlhub_checkout,
        )
    };

    let daemon_base_dir = xmlhub_checkout
        .working_dir_path()
        .append(&*DAEMON_FOLDER_NAME);
    let _ = create_dir(&daemon_base_dir);

    let main_lock_path = (&daemon_base_dir).append("main.lock");
    let get_main_lock = || {
        file_lock_nonblocking(&main_lock_path, true).map_err(|e| match e {
            FileLockError::AlreadyLocked => {
                anyhow!(
                    "xmlhub is already running on this repository, {:?}",
                    xmlhub_checkout.working_dir_path()
                )
            }
            _ => anyhow!("locking {main_lock_path:?}: {e}"),
        })
    };

    if let Some(daemon_mode) = daemon {
        let log_dir = (&daemon_base_dir).append("logs");
        let state_dir = daemon_base_dir;
        let daemon = Daemon {
            state_dir,
            log_dir,
            use_local_time: localtime,
            max_log_file_size: max_log_file_size.unwrap_or(MAX_LOG_FILE_SIZE_DEFAULT),
            max_log_files: Some(max_log_files.unwrap_or(MAX_LOG_FILES_DEFAULT)),
            run: {
                let quietness = quietness.clone();
                move || -> Result<()> {
                    let _main_lock = get_main_lock()?;

                    // Daemon: repeatedly carry out the work by starting a new
                    // child process to do it (so that the child crashing or being
                    // killed due to out of memory conditions does not stop the
                    // daemon).
                    forking_loop(
                        LoopWithBackoff {
                            min_sleep_seconds,
                            max_sleep_seconds: MAX_SLEEP_SECONDS,
                            verbosity: if quietness.quiet() {
                                LoopVerbosity::LogActivityInterval {
                                    every_n_seconds: DAEMON_ACTIVITY_LOG_INTERVAL_SECONDS,
                                }
                            } else {
                                LoopVerbosity::LogEveryIteration
                            },
                            ..Default::default()
                        },
                        // The action run in the child process
                        || {
                            let os = Os::from_local()?;

                            match os {
                                Os::MacOS => {
                                    // Apparently
                                    // setrlimit(Resource::RLIMIT_AS, ) is
                                    // broken on macOS (always returns
                                    // `EINVAL`, and the internet seems to
                                    // indicate that it just doesn't work), thus ignore.
                                }
                                Os::Linux => {
                                    // Set resource limits in case there are issues that
                                    // lead to overuse of CPU or memory
                                    let limit_as =
                                        limit_as.unwrap_or(AS_BYTES_LIMIT_IN_WORKER_CHILD);
                                    setrlimit(Resource::RLIMIT_AS, limit_as, limit_as)
                                        .with_context(|| {
                                            anyhow!("setting RLIMIT_AS to {limit_as}")
                                        })?;
                                }
                            }

                            setrlimit(
                                Resource::RLIMIT_CPU,
                                CPU_SECONDS_LIMIT_IN_WORKER_CHILD,
                                CPU_SECONDS_LIMIT_IN_WORKER_CHILD + 1,
                            )
                            .with_context(|| {
                                anyhow!(
                                "setting RLIMIT_CPU to {CPU_SECONDS_LIMIT_IN_WORKER_CHILD} / {}",
                                CPU_SECONDS_LIMIT_IN_WORKER_CHILD + 1
                            )
                            })?;

                            // Set nicety (scheduling priority):
                            possibly_setpriority(PriorityWhich::Process(0), 10)?;

                            // hack09()?;

                            // Build the index once, throwing away the Ok
                            // return value (replacing it with `()`, since
                            // `forking_loop` expects that (it exits the
                            // child with exit code 0 whenever the action
                            // returned Ok, and that's OK for us, thus we
                            // can and need to drop the code from
                            // `build_index`).
                            build_index_once().map(|_exit_code| ())
                        },
                    )
                }
            },
        };
        daemon.execute(daemon_mode)?;
        std::process::exit(0);
    } else {
        let _main_lock = get_main_lock()?;
        std::process::exit(build_index_once()?);
    }
}

/// Execute a `check` command: prepare and run `build_index` in
/// interactive mode, do not commit. (Never returns `Ok` but exits
/// directly in the non-`Err` case. `!` is not stable yet.)
fn check_command(program_version: GitVersion<SemVersion>, check_opts: CheckOpts) -> Result<()> {
    let CheckOpts {
        versioncheck: VersionCheckOpt { no_version_check },
        dryness: DrynessOpt { dry_run },
        verbosity: VerbosityOpt { verbose },
        quietness,
        file_paths,
        open,
        open_if_changed,
        no_repo_check,
    } = check_opts;
    // What about these?:
    // no_branch_check, -- just use true?
    // ignore_untracked, -- just use true?

    let no_repo_check = typed_from_no_repo_check(no_repo_check);

    let xmlhub_checkouts: Vec<CheckedCheckoutContext1<Cow<Path>>> = file_paths
        .iter()
        .map(|file_path| {
            XMLHUB_CHECKOUT
                .checked_from_subpath(file_path, no_repo_check, false)
                .with_context(|| anyhow!("checking repository for file {file_path:?}"))
        })
        .collect::<Result<_>>()?;

    let git_working_dir = {
        let mut maybe_base_path: Option<&Path> = None;
        for xmlhub_checkout in &xmlhub_checkouts {
            if let Some(path) = maybe_base_path {
                let path2 = xmlhub_checkout.working_dir_path();
                if path != path2 {
                    bail!(
                        "`check` currently needs all FILE_PATHS arguments to be within \
                         the same Git clone"
                    )
                }
            } else {
                maybe_base_path = Some(xmlhub_checkout.working_dir_path());
            }
        }
        let base_path = maybe_base_path
            .ok_or_else(|| anyhow!("`check` needs at least one FILE_PATHS argument"))?;
        GitWorkingDir::from(base_path.to_owned())
    };

    let git_log_version_checker =
        git_log_version_checker(program_version, no_version_check, (&git_working_dir).into());

    let maybe_checked_xmlhub_checkout = None;

    // First, check all the paths are XML files. Partial/adapted copy
    // of the code in build_index.
    let paths: Vec<BaseAndRelPath> = {
        let shared_base_path = git_working_dir.working_dir_path_arc();
        let canonicalized_base_path = shared_base_path
            .canonicalize()
            .with_context(|| anyhow!("canonicalizing the BASE_PATH {shared_base_path:?}"))?;
        file_paths
            .into_iter()
            .map(|file_path| {
                let canonicalized_file_path = file_path
                    .canonicalize()
                    .with_context(|| anyhow!("canonicalizing the file path {file_path:?}"))?;
                let relative_path = canonicalized_file_path
                    .strip_prefix(&canonicalized_base_path)
                    .unwrap_or_else(|_| {
                        panic!(
                            "already checked to be in same repo directory; \
                             file_path = {file_path:?}, \
                             base_path = {shared_base_path:?}"
                        )
                    });
                let barp = BaseAndRelPath::new(
                    Some(Arc::clone(&shared_base_path)),
                    relative_path.to_owned(),
                );
                let is_xml = if let Some(ext) = barp.extension() {
                    ext.eq_ignore_ascii_case("xml")
                } else {
                    false
                };
                if is_xml {
                    Ok(barp)
                } else {
                    bail!("not an XML file path, it does not have a .xml suffix: {file_path:?}")
                }
            })
            .collect::<Result<_>>()?
    };

    // Then run build_index first, because of the run of
    // `git_log_version_checker`, we want that to be done "early", uh,
    // not early anyway. XXX look into when that is called
    // exactly. And XXX using `ok_on_written_errors`, but is that
    // doing all the errors? Relying on that.
    build_index(
        BuildIndexOpts {
            dryness: DrynessOpt { dry_run },
            verbosity: VerbosityOpt { verbose },
            quietness,
            pull: false,
            batch: false,
            ignore_untracked: false,
            write_errors: true,
            silent_on_written_errors: true,
            ok_on_written_errors: true,
            open_if_changed,
            no_commit: true,
            no_commit_errors: true, // but not committing anyway
            no_branch_check: true,  // ?
            open,
        },
        &git_log_version_checker,
        &xmlhub_checkouts[0],
        &maybe_checked_xmlhub_checkout,
    )?;

    // Now check the given paths explicitly.
    let fileinfo_or_errors: Vec<Result<FileInfo<WithExtractedValues>, FileErrors>> =
        read_file_infos(paths);
    let mut exit_code = 0;
    let mut err = stderr().lock();
    let mut hints = Hints::new("checkerror");
    for fileinfo_or_error in fileinfo_or_errors {
        match fileinfo_or_error {
            Ok(fileinfo) => {
                writeln!(
                    &mut err,
                    "    For {:?}: no errors",
                    fileinfo.path.rel_path()
                )?;
            }
            Err(e) => {
                exit_code = 1;
                e.print_plain(&mut hints, &mut err)?;
            }
        }
    }
    hints.print_plain(&mut err)?;
    std::process::exit(exit_code);
}

struct PreparedFile {
    content: String,
    content_has_changed: bool,
    #[allow(unused)]
    data_was_removed: bool,
}

/// Subset of `PrepareOpts`
struct PrepareFileOpts<'t> {
    source_path: &'t Path,
    blinding: &'t BlindingOpts,
    ignore_version: bool,
    quiet: bool,
}

/// Returns the converted file contents, and what changed. Errors
/// already mention the `source_path`.
fn prepare_file(opts: PrepareFileOpts) -> Result<PreparedFile> {
    let PrepareFileOpts {
        source_path,
        blinding:
            BlindingOpts {
                no_blind,
                blind_all,
                blind_comment,
                recommended_max_file_size_bytes,
            },
        ignore_version,
        quiet,
    } = opts;

    let xmldocument = read_xml_file(source_path)
        .with_context(|| anyhow!("loading the XML file {source_path:?}"))?;

    let beast_version = check_beast_version(xmldocument.document(), source_path, ignore_version)
        .with_context(|| anyhow!("preparing the file from {source_path:?}"))?;

    let mut modified_document = ModifiedXMLDocument::new(&xmldocument);

    let document_has_headers = match parse_comments(xmldocument.header_comments(), true) {
        Ok(_) => true,
        Err(_) => false,
    };
    if document_has_headers {
        if !quiet {
            println!("This document already has header comments: {source_path:?}");
        }
    } else {
        // Add header template
        let the_top = modified_document
            .the_top()
            .ok_or_else(|| anyhow!("XML file {source_path:?} gave no top position?"))?;
        modified_document.insert_text_at(the_top.clone(), "\n");
        for spec in METADATA_SPECIFICATION {
            let source_spec = match &spec.source {
                AttributeSource::Specified(source_spec) => source_spec,
                AttributeSource::Derived(_) | AttributeSource::Extracted(_) => continue,
            };
            let comment = format!(
                "{}: {}",
                spec.key.as_ref(),
                if source_spec.need == AttributeNeed::Optional {
                    "NA"
                } else {
                    ""
                }
            );
            modified_document.insert_comment_at(the_top.clone(), &comment, "  ");
        }
        modified_document.insert_text_at(the_top.clone(), "\n");
    }

    // Optionally, delete (blind) data
    let data_was_removed;
    if *no_blind {
        data_was_removed = false;
    } else {
        if beast_version.product != BeastProductVersion::Two {
            bail!(
                "currently, can only blind BEAST 2 files, but this file specifies version {:?}: \
                 {source_path:?} (for BEAST 1 or 3.. files, blind manually or via the \
                 `beast1blinder.py` script and specify the `--no-blind` \
                 option)",
                beast_version.string
            )
        }

        let n_sequences_blinded;
        // Clear the sub-elements, without adding a comment
        if *blind_all {
            // Do blinding below by removing all of "data" element's
            // contents, instead.
            n_sequences_blinded = 0;
        } else {
            let actions = &[ClearAction::Attribute {
                name: "value",
                replacement: "-",
            }];
            // XX should the code check that the `beast > data > sequence`
            // nesting is upheld? This doesn't.
            n_sequences_blinded = modified_document.clear_elements_named(
                SEQUENCES_ELEMENT_NAME,
                &ClearElementsOpts {
                    comment_and_indent: None,
                    always_add_comment: false,
                    actions,
                },
            );
        }

        // Add comment or also clear the element if --blind-all given
        if n_sequences_blinded > 0 || *blind_all {
            let comment = blind_comment
                .as_ref()
                .map(|s| s.as_str())
                .unwrap_or(DEFAULT_COMMENT_FOR_BLINDED_DATA);
            // No actions by default, to just add the comments
            let mut actions = Vec::new();
            if *blind_all {
                actions.push(ClearAction::Element {
                    treat_whitespace_as_empty: true,
                });
            }
            let n = modified_document.clear_elements_named(
                "data",
                &ClearElementsOpts {
                    comment_and_indent: Some((comment, "    ")),
                    // If we're actually blinding here, don't always
                    // add the comment. BTW note: there's a small bug
                    // in that blinding normally first then with
                    // --blind-all adds the comment twice to the
                    // file. I guess that's fitting :)
                    always_add_comment: !*blind_all,
                    actions: &actions,
                },
            );
            if *blind_all {
                data_was_removed = n > 0;
            } else {
                data_was_removed = n_sequences_blinded > 0;
            }
        } else {
            data_was_removed = n_sequences_blinded > 0;
        }
    }

    let (content, content_has_changed) = modified_document.to_string_and_modified()?;

    if data_was_removed && !quiet {
        let more = if *blind_all {
            ""
        } else {
            " Note that sequence metadata has been retained and is assumed \
             to not be privacy sensitive; use `--blind-all` if you want to \
             remove that, too!"
        };
        println!(
            "NOTE: sequences from this document have been removed \
             (use `--no-blind` to keep them!{more}): {source_path:?}"
        );
    }

    let len = modified_document.len()?;
    if len > *recommended_max_file_size_bytes {
        bail!(
            "this file is larger than the recommended maximum file size \
             ({len} > {recommended_max_file_size_bytes} bytes): {source_path:?}.\n\
             Please consider *not* using the `--no-blind` option, or if you \
             are convinced it's worth adding such a large file to the repository, \
             re-run with the `--recommended_max_file_size_bytes` option \
             and a high enough value."
        )
    }

    Ok(PreparedFile {
        content,
        content_has_changed,
        data_was_removed,
    })
}

/// Execute a `prepare` command.
fn prepare_command(command_opts: PrepareOpts) -> Result<()> {
    let PrepareOpts {
        quietness,
        files_to_prepare,
        blinding,
        ignore_version,
    } = command_opts;

    // First, convert them all without writing them out, to avoid
    // writing only some of them (which would then exist when
    // re-running the same command, also it will be a bit
    // confusing). With regards to IO, only reading happens here.
    let converted: Vec<(&PathBuf, PreparedFile)> = files_to_prepare
        .iter()
        .map(|source_path| {
            Ok((
                source_path,
                prepare_file(PrepareFileOpts {
                    source_path,
                    blinding: &blinding,
                    ignore_version,
                    quiet: quietness.quiet(),
                })?,
            ))
        })
        .collect::<Result<_>>()?;

    // Now that all files were read and converted successfully, write
    // them out. With regards to IO, only writing happens here.
    for (target_path, prepared_file) in converted {
        if prepared_file.content_has_changed {
            write_file_moving_to_trash_if_exists(
                &target_path,
                &prepared_file.content,
                quietness.quiet(),
            )?;
        } else {
            if !quietness.quiet() {
                println!("File is unchanged (already prepared): {target_path:?}");
            }
        }
    }
    Ok(())
}

/// Execute an `add-to` command.
fn add_to_command(program_version: GitVersion<SemVersion>, command_opts: AddToOpts) -> Result<()> {
    let AddToOpts {
        versioncheck: VersionCheckOpt { no_version_check },
        quietness,
        blinding,
        target_directory,
        files_to_add,
        mkdir,
        force,
        no_repo_check,
        ignore_version,
    } = command_opts;

    // (Intentionally shadow the original variable to make sure the
    // boolen is never used directly.)
    let no_repo_check = typed_from_no_repo_check(no_repo_check);

    let target_directory = target_directory
        .as_ref()
        .ok_or_else(|| anyhow!("missing TARGET_DIRECTORY argument. Run --help for help."))?;

    if !target_directory.is_dir() {
        if mkdir {
            create_dir(target_directory)
                .with_context(|| anyhow!("creating target directory {target_directory:?}"))?
        } else {
            bail!(
                "given TARGET_DIRECTORY path {target_directory:?} does not exist. \
                 Add the --mkdir option if you want to create it."
            )
        }
    }

    // Check that target_directory or any of the parent directories
    // are an XML Hub clone
    let xmlhub_checkout = XMLHUB_CHECKOUT
        .checked_from_subpath(&target_directory, no_repo_check, false)
        .with_context(|| anyhow!("checking target directory {target_directory:?}"))?;

    // Check that this program is up to date, which matters because
    // otherwise it might add the wrong fields.
    let git_log_version_checker = git_log_version_checker(
        program_version,
        no_version_check,
        xmlhub_checkout.git_working_dir().into(),
    );
    git_log_version_checker.check_git_log()?;

    if files_to_add.is_empty() {
        if !quietness.quiet() {
            println!("No files given, thus nothing to do.");
        }
    } else {
        pluralized! { files_to_add.len() => files }
        if !quietness.quiet() {
            println!("Reading the {files}...");
        }

        // First, convert them all without writing them out, to avoid
        // writing only some of them (which would then exist when
        // re-running the same command, also it will be a bit
        // confusing). With regards to IO, only reading happens here.
        let converted: Vec<_> = files_to_add
            .iter()
            .map(|source_path| {
                Ok((
                    source_path,
                    prepare_file(PrepareFileOpts {
                        source_path,
                        blinding: &blinding,
                        ignore_version,
                        quiet: quietness.quiet(),
                    })?,
                ))
            })
            .collect::<Result<_>>()?;

        // Convert the paths to the output paths; no IO happens here.
        let outputs: Vec<(PathBuf, PreparedFile)> = converted
            .into_iter()
            .map(|(source_path, converted_contents)| -> Result<_> {
                let file_name = source_path
                    .file_name()
                    .with_context(|| anyhow!("given path {source_path:?} is missing file name"))?;
                let target_path = target_directory.append(file_name);
                Ok((target_path, converted_contents))
            })
            .collect::<Result<_>>()?;

        // Stop if any of the files exist, by default.
        if !force {
            let existing_target_paths: Vec<&PathBuf> = outputs
                .iter()
                .filter_map(|(path, _)| if path.exists() { Some(path) } else { None })
                .collect();
            if !existing_target_paths.is_empty() {
                pluralized! { existing_target_paths.len() => these, paths, exist, them }
                bail!(
                    "{these} target {paths} already {exist}, specify the --force option \
                     to overwrite {them}: \n   \
                     {}",
                    existing_target_paths
                        .iter()
                        .map(|s| format!("{s:?}"))
                        .join("\n   ")
                )
            }
        }

        if !quietness.quiet() {
            println!("Writing the {files}...");
        }

        // Now that all files were read, converted and target-checked
        // successfully, write them out. With regards to IO, only writing
        // happens here.
        for (target_path, prepared_file) in &outputs {
            // Note: ignore _modified as that is with regards to the
            // source path, which is a different path. We need to copy the
            // file even if no modification is carried out at the same
            // time!

            // Keep existing files in trash, even with --force?
            write_file_moving_to_trash_if_exists(
                target_path,
                &prepared_file.content,
                quietness.quiet(),
            )?;
        }

        if !quietness.quiet() {
            println!(
                "Done.\n\
                 Now edit the new {files} in {target_directory:?} to complete \
                 the metadata:\n   \
                 {}\n\
                 Run `xmlhub help-attributes` to learn about what to enter into \
                 the individual fields.",
                outputs
                    .iter()
                    .map(|(target_path, _prepared_file)| format!("{target_path:?}"))
                    .join("\n   ")
            );
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    let program_version: GitVersion<SemVersion> = PROGRAM_VERSION
        .parse()
        .with_context(|| anyhow!("the git tag for the release version is not in a valid format"))?;

    // Retrieve the command line options / arguments, and fix
    // those that are overridden by others.
    let opts = {
        // Create an `Opts` from program arguments then deconstruct it
        // immediately, binding the values in the fields to same-named
        // variables, except where followed by `:`, in which case
        // binding the value to a variable with an underscore appended
        // to the field name.
        let Opts {
            v,
            version_only,
            command,
        } = Opts::parse();

        // `--version`
        if v {
            let version_info = VersionInfo::new(&program_version);
            print!("{version_info}");
            return Ok(());
        }
        // `--version-only`
        if version_only {
            println!("{program_version}");
            return Ok(());
        }

        match command {
            Some(command) => match command {
                Command::Build(BuildOpts {
                    dryness,
                    verbosity,
                    versioncheck: VersionCheckOpt { no_version_check },
                    quietness: quietness_,
                    write_errors: write_errors_,
                    no_commit_errors: no_commit_errors_,
                    ok_on_written_errors,
                    silent_on_written_errors: silent_on_written_errors_,
                    open,
                    open_if_changed,
                    pull: pull_,
                    no_commit: no_commit_,
                    push: push_,
                    batch: batch_,
                    no_branch_check,
                    daemon,
                    daemon_sleep_time,
                    base_path,
                    ignore_untracked,
                    no_repo_check,
                    localtime,
                    max_log_file_size,
                    max_log_files,
                    limit_as,
                }) => {
                    // Create uninitialized variables without the underscores,
                    // then initialize them differently depending on some of the
                    // options (--batch, --daemon).
                    let (
                        pull,
                        push,
                        no_commit,
                        write_errors,
                        no_commit_errors,
                        silent_on_written_errors,
                        batch,
                        quietness,
                    );
                    if daemon.is_some() {
                        batch = true;
                    } else {
                        batch = batch_;
                    }
                    if batch {
                        pull = false;
                        push = true;
                        no_commit = false;
                        write_errors = true;
                        no_commit_errors = false;
                        silent_on_written_errors = true;
                        quietness = quietness_.interpret_for_batch_mode();
                        // Should we force `ignore_untracked` false?
                        // No, rather, would want it to be true
                        // because stale files from crashed git runs,
                        // if they are .xml files, would lead to
                        // spurious result. *But*, planning to use
                        // setup for website where there is no
                        // toplevel Git repository, thus wouldn't
                        // work. => Issue to solve in the future.
                    } else {
                        pull = pull_;
                        push = push_;
                        no_commit = no_commit_;
                        write_errors = write_errors_;
                        no_commit_errors = no_commit_errors_;
                        silent_on_written_errors = silent_on_written_errors_;
                        quietness = quietness_;
                    }

                    // Pack the variables into a new struct
                    Opts {
                        v,
                        version_only,
                        command: Some(Command::Build(BuildOpts {
                            dryness,
                            verbosity,
                            versioncheck: VersionCheckOpt { no_version_check },
                            quietness,
                            write_errors,
                            no_commit_errors,
                            ok_on_written_errors,
                            silent_on_written_errors,
                            open,
                            open_if_changed,
                            pull,
                            no_commit,
                            push,
                            batch,
                            daemon,
                            daemon_sleep_time,
                            no_branch_check,
                            ignore_untracked,
                            base_path,
                            no_repo_check,
                            localtime,
                            max_log_file_size,
                            max_log_files,
                            limit_as,
                        })),
                    }
                }
                Command::Install(_)
                | Command::Upgrade(_)
                | Command::CloneTo(_)
                | Command::Prepare(_)
                | Command::AddTo(_)
                | Command::Docs
                | Command::HelpContributing
                | Command::HelpAttributes(_)
                | Command::Check(_)
                | Command::Changelog(_) => Opts {
                    v,
                    version_only,
                    command: Some(command),
                },
            },
            None => {
                bail!("missing command argument. Please run with the `--help` option for help.")
            }
        }
    };

    // Run the requested command
    match opts.command.expect("`None` dispatched already above") {
        Command::Docs => docs_command(program_version),
        Command::HelpContributing => help_contributing_command(),
        Command::HelpAttributes(command_opts) => {
            help_attributes_command(command_opts, program_version)
        }
        Command::Changelog(command_opts) => changelog_command(command_opts),

        Command::Install(command_opts) => install_command(command_opts),
        Command::Upgrade(command_opts) => upgrade_command(program_version, command_opts),

        Command::CloneTo(command_opts) => clone_to_command(program_version, command_opts),

        Command::Prepare(command_opts) => {
            // `prepare` can't check `program_version` as it is not
            // given the path to the repository
            prepare_command(command_opts)
        }
        Command::AddTo(command_opts) => add_to_command(program_version, command_opts),
        Command::Check(command_opts) => check_command(program_version, command_opts),
        Command::Build(command_opts) => build_command(program_version, command_opts),
    }
}
