use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use xmlhub_indexer::clap_styles::clap_styles;
use xmlhub_indexer::get_terminal_width::get_terminal_width;
use xmlhub_indexer::sha256::sha256sum;

#[derive(clap::Parser, Debug)]
#[command(
    next_line_help = true,
    styles = clap_styles(),
    term_width = get_terminal_width(4),
)]
/// Tool to test the sha256 function.
struct Opts {
    file_paths: Vec<PathBuf>,
}

fn main() -> Result<()> {
    let opts: Opts = Opts::parse();
    for path in &opts.file_paths {
        let sum = sha256sum(path).with_context(|| anyhow!("reading file {path:?}"))?;
        println!("{sum}\t{path:?}");
    }
    Ok(())
}
