use std::path::Path;

use crate::{
    git_version::{GitVersion, SemVersion},
    xmlhub_check_version::XmlhubCheckVersion,
    xmlhub_indexer_defaults::XMLHUB_BINARY_FILE_NAME,
    xmlhub_types::OutputFile,
};

/// The index file in HTML format (the one viewed when using `--open`
/// locally).
pub const HTML_FILE: OutputFile = OutputFile {
    path_from_repo_top: "README.html",
};

/// The index file in markdown format (the one viewed on GitLab).
pub const MD_FILE: OutputFile = OutputFile {
    path_from_repo_top: "README.md",
};

/// The name of the command line program.
pub const PROGRAM_NAME: &str = XMLHUB_BINARY_FILE_NAME;

#[derive(clap::Args, Debug)]
pub struct GlobalOpts {
    /// Show external modifying commands that are run. Note that this
    /// does not disable `--quiet`.
    #[clap(short, long)]
    pub verbose: bool,

    /// Suppress some unimportant output; useful with `--daemon` to
    /// reduce the amount of log space required. Note that this does
    /// not disable `--verbose`.
    #[clap(short, long)]
    pub quiet: bool,

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
    pub max_log_files: Option<usize>,

    /// Do not run external processes like git or browsers,
    /// i.e. ignore all the options asking to do so. Instead just say
    /// on stderr what would be done. Still writes to the output
    /// files, though.
    #[clap(long)]
    pub dry_run: bool,

    /// Do not check the program version against versions specified in
    /// the automatic commit messages in the xmlhub repo. Only use if
    /// you know what you're doing.
    #[clap(long)]
    pub no_version_check: bool,
}

pub fn git_log_version_checker(
    program_version: GitVersion<SemVersion>,
    no_version_check: bool,
    base_path: &Path,
) -> XmlhubCheckVersion {
    XmlhubCheckVersion {
        program_name: PROGRAM_NAME,
        program_version: program_version.into(),
        no_version_check,
        base_path: base_path.into(),
        html_file: (&HTML_FILE).into(),
        md_file: (&MD_FILE).into(),
    }
}
