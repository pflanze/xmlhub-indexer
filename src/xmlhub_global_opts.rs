//! Command line option descriptions that are not actually global
//! program options, but options that are used across multiple
//! subcommands.

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

#[derive(clap::Args, Debug)]
pub struct BlindingOpts {
    /// Do *not* strip the sequences; by default they are stripped
    /// (the values of `value` attributes of `<sequence>` elements),
    /// as safety measure to avoid accidental exposure of private
    /// data. If you can publish the data and it's not overly large,
    /// you're encouraged to use this option! Also see the
    /// `--blind-all` option.
    #[clap(long)]
    pub no_blind: bool,

    /// When stripping sequence data (i.e. no `--no-blind` option was
    /// given), strip the whole `<data>` element contents instead of
    /// just the `value` attributes of `<sequence>` elements. This
    /// will also remove sequence metadata, which may be necessary if
    /// your metadata is privacy sensitive, but it will create a file
    /// that cannot be run in BEAST2.
    #[clap(long)]
    pub blind_all: bool,

    /// The comment to put above `<data>` elements when blinding the
    /// data (i.e. `--no-blind` is not given). By default, a comment
    /// with regards to terms of use and privacy is given.
    #[clap(long)]
    pub blind_comment: Option<String>,

    /// Contributed files (as added to Git) should be smaller than
    /// this. `xmlhub` will refuse to accept files larger than
    /// this. If you want to add files larger than this, you can
    /// either specify a large enough size here, or you may want to
    /// decide to allow `xmlhub prepare` to blind the data so that the
    /// file gets smaller. The size is checked after preparing the
    /// file, though; if the file is still too large even after
    /// blinding, you may want to use the `--blind-all` option or find
    /// out why your file is so large.
    #[clap(long, default_value = "5000000")]
    pub recommended_max_file_size_bytes: usize,
}
