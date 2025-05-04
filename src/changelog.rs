use std::{borrow::Cow, fmt::Display, mem::take};

use anyhow::{anyhow, bail, Context, Result};

use crate::{
    git_version::{GitVersion, SemVersion},
    ref_or_owned::RefOrOwned,
};

pub const CHANGELOG_FILE_NAME: &str = "Changelog.md";
pub const CHANGELOG: &str = include_str!("../Changelog.md");

#[derive(Clone, Debug)]
pub struct Release<'t> {
    version: RefOrOwned<'t, GitVersion<SemVersion>>,
    date: RefOrOwned<'t, String>,
}

#[derive(Clone, Debug)]
pub enum ChangelogEntry<'s, 't> {
    Release(Release<'t>),
    PointEntry(&'s str),
}

#[derive(Clone, Debug)]
pub struct Changelog<'s: 't, 't, 't0: 't> {
    /// When a subset was taken, what the range was
    pub from: Option<RefOrOwned<'t, GitVersion<SemVersion>>>,
    /// Whether `from` is included (inclusive range)
    pub include_from: bool,
    pub to: Option<RefOrOwned<'t, GitVersion<SemVersion>>>,
    pub is_downgrade: bool,

    /// The title from the Changelog.md, not used.
    pub title: Option<&'s str>,
    /// The "Newest" sentence from the Changelog.md, used in
    /// innovative output style.
    pub newest: Option<&'s str>,
    pub entries: Cow<'t, [ChangelogEntry<'s, 't0>]>,
}

#[derive(Clone, Copy, Debug)]
pub enum ChangelogDisplayStyle {
    /// Use the innovative(TM) ordering as in the source file
    /// (Changelog.md).
    Innovative,

    /// Whether to show release tags and dates as section titles with
    /// the changes for that release in the section body (i.e. more
    /// traditional).
    ReleasesAsSections {
        /// Whether to print a ":" after a section header
        print_colon_after_release: bool,

        /// Whether to show newest sections at the top (i.e. more
        /// traditional). (`true` for this might be less confusing.)
        newest_section_first: bool,

        /// Whether to sort newest items within a section first. (Not
        /// sure what the better value is for this.)
        newest_item_first: bool,
    },
}

impl ChangelogDisplayStyle {
    pub fn is_innovative(&self) -> bool {
        match self {
            ChangelogDisplayStyle::Innovative => true,
            ChangelogDisplayStyle::ReleasesAsSections {
                print_colon_after_release: _,
                newest_section_first: _,
                newest_item_first: _,
            } => false,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ChangelogDisplay<'s: 't, 't, 't0: 't, 't1> {
    pub changelog: &'t1 Changelog<'s, 't, 't0>,
    pub generate_title: bool,
    pub style: ChangelogDisplayStyle,
}

impl<'s: 't, 't, 't0: 't, 't1> Display for ChangelogDisplay<'s, 't, 't0, 't1> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ChangelogDisplay {
            changelog,
            generate_title,
            style,
        } = self;

        if *generate_title {
            f.write_str(&changelog.display_title())?;
        }

        match style {
            ChangelogDisplayStyle::Innovative => {
                if let Some(newest) = changelog.newest {
                    writeln!(f, "{newest}\n")?;
                }
                for entry in &*changelog.entries {
                    match entry {
                        ChangelogEntry::Release(Release { version, date }) => {
                            writeln!(f, "\nv{version} released on {date}\n")?
                        }
                        ChangelogEntry::PointEntry(e) => writeln!(f, "{e}")?,
                    }
                }
                Ok(())
            }

            ChangelogDisplayStyle::ReleasesAsSections {
                print_colon_after_release,
                newest_section_first,
                newest_item_first,
            } => {
                let mut sections = changelog.sections();
                if *newest_section_first {
                    sections.reverse();
                }

                let possibly_colon = if *print_colon_after_release { ":" } else { "" };

                for section in &sections {
                    let ChangelogSection { release, entries } = section;
                    if let Some(Release { version, date }) = release {
                        writeln!(f, "\n## v{version} ({date}){possibly_colon}\n",)?;
                    } else {
                        writeln!(f, "\n## (unreleased){possibly_colon}\n")?;
                    }
                    let entries: Box<dyn Iterator<Item = &&str>> = if *newest_item_first {
                        Box::new(entries.iter().rev())
                    } else {
                        Box::new(entries.iter())
                    };
                    for e in entries {
                        writeln!(f, "{e}")?;
                    }
                }
                Ok(())
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct ChangelogSection<'s, 't0> {
    pub release: Option<Release<'t0>>,
    pub entries: Vec<&'s str>,
}

#[derive(thiserror::Error, Debug)]
pub enum ChangelogGetError {
    #[error("given `from` release number is after `to`: {0} > {1}")]
    FromAfterTo(String, String),
    #[error(
        "Changelog.md has wrongly ordered releases, \
         or there is a bug: expected {0} < {1}"
    )]
    ChangelogFileHasWronglyOrderedReleases(String, String),
}

impl<'t, 't0> Changelog<'static, 't, 't0> {
    pub fn new_builtin() -> Result<Self> {
        Changelog::from_str(CHANGELOG)
    }
}

// Do not impl `Display` as we want the `ChangelogDisplay` options,
// and don't want to make that part of `Changelog`.
impl<'s: 't, 't, 't0> Changelog<'s, 't, 't0> {
    pub fn sections(&'t self) -> Vec<ChangelogSection<'s, 't>> {
        let mut sections = Vec::new();
        let mut entries: Vec<&'s str> = Vec::new();
        for entry in &*self.entries {
            match entry {
                ChangelogEntry::Release(Release { version, date }) => {
                    let entries = take(&mut entries);
                    sections.push(ChangelogSection {
                        release: Some(Release {
                            version: version.as_ref().into(),
                            date: date.as_ref().into(),
                        }),
                        entries,
                    });
                }
                ChangelogEntry::PointEntry(e) => entries.push(*e),
            }
        }

        if !entries.is_empty() {
            sections.push(ChangelogSection {
                release: None,
                entries,
            });
        }
        sections
    }

    pub fn display_title(&self) -> String {
        let from_or_since = if self.include_from { "from" } else { "since" };
        let from_or_since_from = self
            .from
            .as_ref()
            .map(|v| format!("{from_or_since} version {v}"))
            .unwrap_or("".into());
        let until_to = self
            .to
            .as_ref()
            .map(|v| format!("until version {v}"))
            .unwrap_or("".into());
        format!(
            "# Changes {from_or_since_from} {until_to}{}",
            if self.is_downgrade {
                " (for downgrade)"
            } else {
                ""
            }
        )
    }

    pub fn from_str(changelog: &'s str) -> Result<Self> {
        let mut title = None;
        let mut newest = None;
        let mut entries = Vec::new();
        let mut lineno = 0;
        for line in changelog.split('\n') {
            lineno += 1;
            match line.chars().next() {
                None => (),
                Some('#') => title = Some(line),
                Some('v') => {
                    let parts: Vec<&str> = line.split(" - ").collect();
                    if let [version_str, date_str] = parts.as_slice() {
                        let version: GitVersion<SemVersion> =
                            version_str.trim().parse().with_context(|| {
                                anyhow!("parsing version number {line:?} on line {lineno}")
                            })?;
                        let date = date_str.to_string();
                        entries.push(ChangelogEntry::Release(Release {
                            version: version.into(),
                            date: date.into(),
                        }));
                    } else {
                        bail!(
                            "expecting 2 parts in a release line split on ' - ', on line {lineno}"
                        )
                    }
                }
                Some('-') => entries.push(ChangelogEntry::PointEntry(line)),
                _ => {
                    if line.starts_with("Newest") {
                        newest = Some(line)
                    } else if line.starts_with("cj")
                        || line.starts_with("Versions")
                        || line.starts_with("...")
                    {
                        ()
                    } else if line.chars().all(|c| c.is_whitespace()) {
                        ()
                    } else {
                        bail!("can't parse line {line:?} on line {lineno}")
                    }
                }
            }
        }
        Ok(Self {
            title,
            newest,
            entries: entries.into(),
            include_from: true,
            from: None.into(),
            to: None.into(),
            is_downgrade: false,
        })
    }

    /// Select a sub-range of changes between two given versions, or
    /// if None, the beginning or end of the whole changelog,
    /// respectively. `include_from` indicates whether the from
    /// release line should be included (but without its items!) or
    /// not. If `allow_downgrades` is false, gives an error if `from`
    /// > `to`.
    pub fn get_between_versions<'slf>(
        &'slf self,
        allow_downgrades: bool,
        include_from: bool,
        // evil to use 'slf here?
        from: Option<&'slf GitVersion<SemVersion>>,
        to: Option<&'slf GitVersion<SemVersion>>,
    ) -> Result<Self, ChangelogGetError>
    where
        'slf: 't, // ah, because Cow may own the storage, then referncing it is 'slf not 't
    {
        let is_downgrade = {
            let mut is_downgrade = false;
            if let Some(from) = from {
                if let Some(to) = to {
                    if from > to {
                        if !allow_downgrades {
                            return Err(ChangelogGetError::FromAfterTo(
                                format!("{from}"),
                                format!("{to}"),
                            ));
                        }
                        is_downgrade = true;
                    }
                }
            }
            is_downgrade
        };

        let (from, to) = if is_downgrade { (to, from) } else { (from, to) };

        let (possibly_after_start, after_end) = {
            let len = self.entries.len();
            let mut start = if from.is_some() { None } else { Some(0) };
            let mut end = if to.is_some() { None } else { Some(len - 1) };
            for i in 0..len {
                let entry = &self.entries[i];
                match entry {
                    ChangelogEntry::Release(Release { version, date: _ }) => {
                        if let Some(from) = from {
                            if start.is_none() {
                                if **version >= *from {
                                    start = Some(i);
                                }
                            }
                        }
                        if let Some(to) = to {
                            if end.is_none() {
                                if **version >= *to {
                                    end = Some(i);
                                }
                            }
                        }
                        if start.is_some() && end.is_some() {
                            break;
                        }
                    }
                    ChangelogEntry::PointEntry(_) => (),
                }
            }
            let start = start.unwrap_or(len);
            let possibly_after_start = if include_from {
                start
            } else {
                if let Some(entry) = self.entries.get(start) {
                    match entry {
                        ChangelogEntry::Release(_) => start.saturating_add(1).min(len),
                        ChangelogEntry::PointEntry(_) => start,
                    }
                } else {
                    start
                }
            };
            let end = end.unwrap_or(len);
            let after_end = end.saturating_add(1).min(len);
            (possibly_after_start, after_end)
        };

        if possibly_after_start > after_end {
            return Err(ChangelogGetError::ChangelogFileHasWronglyOrderedReleases(
                format!("{from:?}"),
                format!("{to:?}"),
            ));
        }

        Ok(Self {
            include_from,
            from: from.map(RefOrOwned::from),
            to: to.map(RefOrOwned::from),
            is_downgrade,
            title: self.title,
            newest: self.newest,
            entries: (&self.entries[possibly_after_start..after_end]).into(),
        })
    }
}

#[test]
fn t_changelog() -> Result<()> {
    use std::str::FromStr;
    let changelog = Changelog::new_builtin()?;
    let from = GitVersion::from_str("v1.2")?;
    let to = GitVersion::from_str("v6")?;
    let sublog = changelog.get_between_versions(false, true, Some(&from), Some(&to))?;
    assert!(changelog.entries.len() > sublog.entries.len());
    Ok(())
}
