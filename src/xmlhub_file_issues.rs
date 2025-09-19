// Error reporting: this program does not stop but continues
// processing when it encounters errors, collecting them and then in
// the end reporting them all (both on the command line and in the
// output page).

use std::io::Write;

use ahtml::{att, flat::Flat, util::SoftPre, HtmlAllocator, Node};
use anyhow::Result;
use run_git::git::BaseAndRelPath;

use crate::xmlhub_indexer_defaults::document_symbol;

/// An error report with all errors that happened while processing one
/// particular file. An error prevents the file from being included in
/// the index. They are shown in the list of errors and warnings
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

    fn info_box_id(&self) -> Option<usize> {
        None
    }
}

/// A report with all warnings that happened while processing one
/// particular file. A warning does not prevent the file from being
/// included in the index; they are shown with the file info box and
/// also in the list of errors and warnings. But this is only used
/// temporarily to share the infrastructure here.
#[derive(Debug)]
pub struct FileWarnings<'t> {
    pub path: &'t BaseAndRelPath,
    pub id: usize,
    pub warnings: &'t Vec<String>,
}

impl<'t> FileIssues for FileWarnings<'t> {
    fn rel_path(&self) -> &str {
        self.path.rel_path()
    }

    fn issues(&self) -> &[String] {
        &self.warnings
    }

    fn info_box_id(&self) -> Option<usize> {
        Some(self.id)
    }
}

pub trait FileIssues {
    fn rel_path(&self) -> &str;
    fn issues(&self) -> &[String];
    /// id for linking to html box (fallback is to link to the document itself
    /// via rel_path)
    fn info_box_id(&self) -> Option<usize>;

    fn is_empty(&self) -> bool {
        self.issues().is_empty()
    }

    /// Returns `<dt>..<dd>..` (definition term / definition data)
    /// pairs to be used in a `<dl>..</dl>` (definition list).
    fn to_html(
        &self,
        show_path: bool,
        info_box_id_prefix: &str,
        html: &HtmlAllocator,
    ) -> Result<Flat<Node>> {
        const SOFT_PRE: SoftPre = SoftPre {
            tabs_to_nbsp: Some(4),
            autolink: true,
            input_line_separator: "\n",
            trailing_br: false,
        };

        let mut dt_body = html.new_vec();
        if show_path {
            dt_body.push(html.text("For ")?)?;
            if let Some(info_box_id) = self.info_box_id() {
                dt_body.push(html.a(
                    [
                        att("href", format!("#{info_box_id_prefix}-{info_box_id}")),
                        att("title", "Jump to info box"),
                    ],
                    html.text(self.rel_path())?,
                )?)?;
                dt_body.push(html.nbsp()?)?;
                dt_body.push(html.a(
                    [att("href", self.rel_path()), att("title", "Open the file")],
                    document_symbol(html)?,
                )?)?;
            } else {
                dt_body.push(html.a(
                    [att("href", self.rel_path()), att("title", "Open the file")],
                    [
                        html.text(self.rel_path())?,
                        html.nbsp()?,
                        document_symbol(html)?,
                    ],
                )?)?;
            }
            dt_body.push(html.text(":")?)?;
        }

        let mut ul_body = html.new_vec();
        for issue in self.issues() {
            ul_body.push(html.li([], SOFT_PRE.format(issue, html)?)?)?;
        }

        let dt = html.dt([], dt_body)?;
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
