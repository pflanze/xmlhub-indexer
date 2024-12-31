use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

use xmlhub_indexer::{
    git::GitLogEntry,
    git_check_version::git_check_version,
    git_version::{GitVersion, SemVersion},
};

#[derive(clap::Parser, Debug)]
/// Test program version checking from Git.
struct Opts {
    base_path: PathBuf,
    our_version: GitVersion<SemVersion>,
}

fn parse_version(entry: &GitLogEntry) -> Option<GitVersion<SemVersion>> {
    let lines: Vec<&str> = entry.message.split('\n').collect();

    let subject = lines.get(0)?;
    if !subject.contains("xmlhub-indexer") {
        return None;
    }

    let empty = lines.get(1)?;
    if !empty.is_empty() {
        return None;
    }

    let body1 = lines.get(2)?;
    let body_key = "version:";
    if body1.starts_with(body_key) {
        let version_str = body1[body_key.as_bytes().len()..].trim();
        version_str.parse().ok()
    } else {
        None
    }
}

fn main() -> Result<()> {
    let opts = Opts::from_args();

    let ordering = git_check_version(
        &opts.base_path,
        &["README.html", "README.md"],
        parse_version,
        &opts.our_version,
    )?;
    if let Some(ordering) = ordering {
        println!(
            "all is ok, program version is {ordering:?} than \
             the found version of the data"
        );
    } else {
        println!("no entries found, assuming that all is ok.");
    }
    Ok(())
}
