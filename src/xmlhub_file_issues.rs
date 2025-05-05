// Error reporting: this program does not stop but continues
// processing when it encounters errors, collecting them and then in
// the end reporting them all (both on the command line and in the
// output page).

use std::io::Write;

use ahtml::{att, flat::Flat, util::SoftPre, HtmlAllocator, Node};
use anyhow::Result;

use crate::{git::BaseAndRelPath, xmlhub_indexer_defaults::document_symbol};

/// An error report with all errors that happened while processing one
/// particular file. An error prevents the file from being included in
/// the index.
#[derive(Debug)]
pub struct FileErrors {
    pub path: BaseAndRelPath,
    pub errors: Vec<String>,
}

impl FileIssues for FileErrors {
    fn rel_path(&self) -> &str {
        self.path.rel_path()
    }

    fn issues(&self) -> &[String] {
        &self.errors
    }
}

pub trait FileIssues {
    fn rel_path(&self) -> &str;
    fn issues(&self) -> &[String];

    /// Returns `<dt>..<dd>..` (definition term / definition data)
    /// pairs to be used in a `<dl>..</dl>` (definition list).
    fn to_html(&self, html: &HtmlAllocator) -> Result<Flat<Node>> {
        const SOFT_PRE: SoftPre = SoftPre {
            tabs_to_nbsp: Some(4),
            autolink: true,
            input_line_separator: "\n",
        };
        let mut ul_body = html.new_vec();
        for issue in self.issues() {
            ul_body.push(html.li([], SOFT_PRE.format(issue, html)?)?)?;
        }
        let dt = html.dt(
            [],
            [
                html.text("For ")?,
                html.a(
                    [att("href", self.rel_path()), att("title", "Open the file")],
                    [
                        html.text(self.rel_path())?,
                        html.nbsp()?,
                        document_symbol(html)?,
                    ],
                )?,
                html.text(":")?,
            ],
        )?;
        let dd = html.dd([], html.ul([], ul_body)?)?;
        Ok(Flat::Two(dt, dd))
    }

    /// Print as plaintext, for error reporting to stderr.
    fn print_plain<O: Write>(&self, out: &mut O) -> Result<()> {
        writeln!(out, "    For {:?}:", self.rel_path())?;
        for issue in self.issues() {
            let lines: Vec<&str> = issue.split('\n').collect();
            writeln!(out, "      * {}", lines[0])?;
            for line in &lines[1..] {
                writeln!(out, "        {}", line)?;
            }
        }
        Ok(())
    }
}
