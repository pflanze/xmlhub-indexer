use std::fmt::Display;

use ahtml::{att, AId, HtmlAllocator, Node};
use anyhow::Result;

use crate::git_version::{GitVersion, SemVersion};

/// Version and build information about this program.
pub struct VersionInfo(Vec<(Option<&'static str>, String)>);

impl VersionInfo {
    pub fn new(program_version: &GitVersion<SemVersion>) -> Self {
        let mut info = Vec::new();

        info.push((None, format!("{program_version}")));

        info.push((
            Some("Compiled for OS/architecture"),
            format!("{}/{}", std::env::consts::OS, std::env::consts::ARCH),
        ));

        let profile = if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        };
        info.push((Some("Compilation profile"), format!("{profile}")));

        Self(info)
    }

    pub fn to_html(&self, html: &HtmlAllocator) -> Result<AId<Node>> {
        let mut rows = html.new_vec();
        for (key, val) in &self.0 {
            rows.push(html.tr(
                [],
                [
                    html.td(
                        [att("style", "padding-right: 1em; font-style: italic;")],
                        [html.text(key.unwrap_or("Version"))?, html.text(":")?],
                    )?,
                    html.td([], html.text(val)?)?,
                ],
            )?)?;
        }
        html.table([att("border", "0")], rows)
    }
}

impl Display for VersionInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (key, val) in &self.0 {
            if let Some(key) = key {
                f.write_str(key)?;
                f.write_str(": ")?;
            }
            f.write_str(val)?;
            f.write_str("\n")?;
        }
        Ok(())
    }
}
