//! Not actually global options, but options that are used across
//! multiple subcommands.

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

#[derive(clap::Args, Debug, Clone)]
pub struct VerbosityOpt {
    /// Show external modifying commands that are run. (Note that this
    /// does not disable `--quiet` if that option is allowed.)
    #[clap(short, long)]
    pub verbose: bool,
}

#[derive(clap::Args, Debug)]
pub struct QuietOpt {
    /// Suppress some unimportant output. (Note that this does
    /// not disable `--verbose` if that option is allowed.)
    #[clap(short, long)]
    pub quiet: bool,
}

#[derive(clap::Args, Debug, Clone)]
pub struct DrynessOpt {
    /// Do not run external processes like git or browsers,
    /// i.e. ignore all the options asking to do so. Instead just say
    /// on stderr what would be done. Still writes to the output
    /// files, though.
    #[clap(long)]
    pub dry_run: bool,
}

#[derive(clap::Args, Debug)]
pub struct VersionCheckOpt {
    /// Do not check the program version against versions specified in
    /// the automatic commit messages in the xmlhub repo. Only use if
    /// you know what you're doing.
    #[clap(long)]
    pub no_version_check: bool,
}

#[derive(clap::Args, Debug)]
pub struct OpenOrPrintOpts {
    /// Show the output in the browser (default)
    #[clap(long)]
    open: bool,
    /// Print the output to the terminal in Markdown format instead of
    /// showing it in the browser (although if `--open` is given
    /// explicitly, this is still done, too)
    #[clap(long)]
    print: bool,
}

impl OpenOrPrintOpts {
    pub fn do_opts(&self) -> (bool, bool) {
        let Self { open, print } = self;
        let (do_open, do_print) = match (open, print) {
            (false, false) => (true, false),
            _ => (*open, *print),
        };
        (do_open, do_print)
    }

    pub fn do_open(&self) -> bool {
        self.do_opts().0
    }

    pub fn do_print(&self) -> bool {
        self.do_opts().1
    }
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
