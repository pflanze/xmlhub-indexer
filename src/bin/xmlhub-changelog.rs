use anyhow::Result;
use clap::Parser;
use xmlhub_indexer::changelog::{Changelog, ChangelogDisplay, ChangelogDisplayStyle};
use xmlhub_indexer::clap_styles::clap_styles;
use xmlhub_indexer::get_terminal_width::get_terminal_width;
use xmlhub_indexer::git_version::{GitVersion, SemVersion};

#[derive(clap::Parser, Debug)]
#[command(
    next_line_help = true,
    styles = clap_styles(),
    term_width = get_terminal_width(4),
)]
/// Tool to test the changelog functionality.
struct Opts {
    /// Whether to include the starting release (if any) in the output
    #[clap(long)]
    include_from: bool,
    #[clap(long)]
    newest_section_first: bool,
    #[clap(long)]
    newest_item_first: bool,
    #[clap(long)]
    as_sections: bool,
    #[clap(long)]
    allow_downgrades: bool,

    #[clap(long)]
    from: Option<GitVersion<SemVersion>>,
    #[clap(long)]
    to: Option<GitVersion<SemVersion>>,
}

fn main() -> Result<()> {
    let opts: Opts = Opts::parse();
    let changelog = Changelog::new_builtin()?;
    let part = changelog.get_between_versions(
        opts.allow_downgrades,
        opts.include_from,
        opts.from.as_ref(),
        opts.to.as_ref(),
    )?;

    print!(
        "{}",
        ChangelogDisplay {
            changelog: &part,
            generate_title: true,
            style: if opts.as_sections {
                ChangelogDisplayStyle::ReleasesAsSections {
                    print_colon_after_release: true,
                    newest_section_first: opts.newest_section_first,
                    newest_item_first: opts.newest_item_first,
                }
            } else {
                ChangelogDisplayStyle::Innovative
            },
        }
    );
    Ok(())
}
