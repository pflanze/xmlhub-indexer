use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

use xmlhub_indexer::{cargo::check_cargo_toml_no_path, get_terminal_width::get_terminal_width};

#[derive(clap::Parser, Debug)]
#[clap(next_line_help = true)]
#[clap(set_term_width = get_terminal_width())]
/// Just testing cargo.rs library interactively
struct Opts {
    /// Paths to test
    paths: Vec<PathBuf>,
}

fn main() -> Result<()> {
    let opts = Opts::parse();

    for path in opts.paths {
        check_cargo_toml_no_path(&path)?;
    }

    Ok(())
}
