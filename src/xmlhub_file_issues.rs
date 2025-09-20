// Error reporting: this program does not stop but continues
// processing when it encounters errors, collecting them and then in
// the end reporting them all (both on the command line and in the
// output page).

use std::io::Write;

use ahtml::{att, flat::Flat, util::SoftPre, HtmlAllocator, Node};
use anyhow::Result;
use run_git::git::BaseAndRelPath;

use crate::{hints::Hints, xmlhub_fileinfo::Issue, xmlhub_indexer_defaults::document_symbol};

/// An error report with all errors that happened while processing one
/// particular file. An error prevents the file from being included in
/// the index. They are shown in the list of errors and warnings
#[derive(Debug)]
pub struct FileErrors {
    pub path: BaseAndRelPath,
    pub errors: Vec<Issue>,
}

impl FileIssues for FileErrors {
    fn rel_path(&self) -> &str {
        self.path.rel_path()
    }

    fn issues(&self) -> &[Issue] {
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
    pub warnings: &'t Vec<Issue>,
}

impl<'t> FileIssues for FileWarnings<'t> {
    fn rel_path(&self) -> &str {
        self.path.rel_path()
    }

    fn issues(&self) -> &[Issue] {
        &self.warnings
    }

    fn info_box_id(&self) -> Option<usize> {
        Some(self.id)
    }
}

pub trait FileIssues {
    fn rel_path(&self) -> &str;
    fn issues(&self) -> &[Issue];
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
        hints: &mut Hints,
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
        for Issue { message, hint } in self.issues() {
            let msg_html = SOFT_PRE.format(message, html)?;
            let item_html = if let Some(hint) = hint {
                Flat::Two(msg_html, hints.intern(hint.clone()).to_html(html)?)
            } else {
                Flat::One(msg_html)
            };
            ul_body.push(html.li([], item_html)?)?;
        }
        let dt = html.dt([], dt_body)?;
        let dd = html.dd([], html.ul([], ul_body)?)?;
        Ok(Flat::Two(dt, dd))
    }

    /// Print as plaintext, for error reporting to stderr.
    fn print_plain<O: Write>(&self, hints: &mut Hints, out: &mut O) -> Result<()> {
        writeln!(out, "    For {:?}:", self.rel_path())?;
        for Issue { message, hint } in self.issues() {
            let hint_ref_str = if let Some(hint) = hint {
                hints.intern(hint.clone()).to_plain()
            } else {
                "".into()
            };
            let lines: Vec<&str> = message.split('\n').collect();
            for (i, line) in lines.iter().enumerate() {
                let is_first = i == 0;
                let is_last = i == lines.len() - 1;

                let prefix = if is_first { "      * " } else { "        " };
                let postfix = if is_last { &hint_ref_str } else { "" };
                writeln!(out, "{prefix}{}{postfix}", line)?;
            }
        }
        Ok(())
    }
}
