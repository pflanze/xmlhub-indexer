// Error reporting: this program does not stop but continues
// processing when it encounters errors, collecting them and then in
// the end reporting them all (both on the command line and in the
// output page).

use std::io::Write;

use ahtml::{att, flat::Flat, util::SoftPre, HtmlAllocator, Node};
use anyhow::Result;

use crate::{git::BaseAndRelPath, xmlhub_indexer_defaults::document_symbol};

/// An error report with all errors that happened while processing one
/// particular file.
#[derive(Debug)]
pub struct FileErrors {
    pub path: BaseAndRelPath,
    pub errors: Vec<String>,
}

impl FileErrors {
    /// Returns `<dt>..<dd>..` (definition term / definition data)
    /// pairs to be used in a `<dl>..</dl>` (definition list).
    pub fn to_html(&self, html: &HtmlAllocator) -> Result<Flat<Node>> {
        const SOFT_PRE: SoftPre = SoftPre {
            tabs_to_nbsp: Some(4),
            autolink: true,
            input_line_separator: "\n",
        };
        let mut ul_body = html.new_vec();
        for error in &self.errors {
            ul_body.push(html.li([], SOFT_PRE.format(error, html)?)?)?;
        }
        let dt = html.dt(
            [],
            [
                html.text("For ")?,
                html.a(
                    [
                        att("href", self.path.rel_path()),
                        att("title", "Open the file"),
                    ],
                    [
                        html.text(self.path.rel_path())?,
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
    pub fn print_plain<O: Write>(&self, out: &mut O) -> Result<()> {
        writeln!(out, "    For {:?}:", self.path.rel_path())?;
        for error in &self.errors {
            let lines: Vec<&str> = error.split('\n').collect();
            writeln!(out, "      * {}", lines[0])?;
            for line in &lines[1..] {
                writeln!(out, "        {}", line)?;
            }
        }
        Ok(())
    }
}
