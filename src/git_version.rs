use std::{
    cmp::Ordering,
    fmt::{Debug, Display, Write},
    str::FromStr,
};

use anyhow::{anyhow, bail, Context, Result};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Error)]
pub enum UndecidabilityReason {
    #[error(
        "one or both versions represent work in progress and it is \
         unknown if one is a parent of the other"
    )]
    Wip(bool, bool),
    #[error("the left version is missing a value")]
    LeftMissing,
    #[error("the right version is missing a value")]
    RightMissing,
    // Not sure this case can ever happen, but let's play safe:
    #[error("both versions are missing a value")]
    BothMissing,
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum SemVerOrdResult {
    #[error("the two versions are compatible regardless of the ordering")]
    Equivalent(Ordering),
    #[error("the versions are upgrade compatible, according to the given ordering")]
    Upgrade(Ordering),
    #[error("the given ordering {1:?} is not reliable because {0}")]
    Undecidable(UndecidabilityReason, Ordering),
    #[error("`partial_cmp` between these two values returned `None`: {0}")]
    FailedPartialOrd(Box<String>),
}

/// Unlike `PartialOrd` or `Ord`, `SemVerOrd` reports whether two
/// versions are compatible (`Ordering::Equal`), and if not, which one
/// is the higher version (e.g. to allow a program to upgrade data
/// when the version is newer, but not when it is older).
pub trait SemVerOrd {
    fn semver_cmp(&self, other: &Self) -> SemVerOrdResult;
}

/// Represent a version number as given by `git describe`, or rather,
/// the `git-version` crate (i.e. allows a `-modified` suffix--this is
/// untested!).
#[derive(Debug, PartialEq, Clone)]
pub struct GitVersion<V: Debug> {
    pub version: V,
    pub past_tag: Option<(u32, String)>,
    pub modified: bool,
}

impl<V: Display + Debug> Display for GitVersion<V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.version, f)?;
        if let Some((depth, hash)) = &self.past_tag {
            f.write_fmt(format_args!("-{depth}-g{hash}"))?;
        }
        if self.modified {
            // XX is this the correct place?
            f.write_str("-modified")?;
        }
        Ok(())
    }
}

impl<V: FromStr + Debug> FromStr for GitVersion<V>
where
    anyhow::Error: From<<V as FromStr>::Err>,
{
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<_> = s.split('-').collect();
        let parse_version = || {
            parts[0]
                .parse()
                .map_err(anyhow::Error::from)
                .with_context(|| {
                    anyhow!(
                        "expecting a version number string consisting of the optional \
                         letter 'v' followed by 1-3 non-negative integer numbers \
                         with '.' inbetween, got {:?}",
                        parts[0]
                    )
                })
        };
        let parse = |modified| {
            let depth = parts[1]
                .parse()
                .with_context(|| anyhow!("expecting unsigned integer, got {:?}", parts[1]))?;
            if !parts[2].starts_with('g') {
                bail!("expecting `g...`, got {:?} in {s:?}", parts[2])
            }
            Ok(GitVersion {
                version: parse_version()?,
                past_tag: Some((depth, parts[2][1..].into())),
                modified,
            })
        };
        match parts.len() {
            1 => Ok(GitVersion {
                version: parse_version()?,
                past_tag: None,
                modified: false,
            }),
            2 if parts[1] == "modified" => Ok(GitVersion {
                version: parse_version()?,
                past_tag: None,
                modified: true,
            }),
            3 => parse(false),
            4 if parts[3] == "modified" => parse(true),
            _ => bail!(
                "expecting either no '-' or two of them and optionally with \
                 `-modified` appended, but got: {s:?}"
            ),
        }
    }
}

#[cfg(test)]
#[test]
fn t_git_version_string() {
    let t = |s: &str| -> GitVersion<String> { s.parse().unwrap() };
    let t_err =
        |s: &str| -> String { GitVersion::<String>::from_str(s).err().unwrap().to_string() };
    assert_eq!(
        t("1.2.3-7-g8c847ab"),
        GitVersion {
            version: "1.2.3".into(),
            past_tag: Some((7, "8c847ab".into())),
            modified: false
        }
    );
    // XX is this how the git-version crate prints it? todo test.
    assert_eq!(
        t("1.2.3-7-g8c847ab-modified"),
        GitVersion {
            version: "1.2.3".into(),
            past_tag: Some((7, "8c847ab".into())),
            modified: true
        }
    );
    assert_eq!(
        t("1.2.3"),
        GitVersion {
            version: "1.2.3".into(),
            past_tag: None,
            modified: false
        }
    );
    assert_eq!(
        t("1.2.3-modified"),
        GitVersion {
            version: "1.2.3".into(),
            past_tag: None,
            modified: true
        }
    );
    assert_eq!(
        t_err("1.2.3-modified-324"),
        "expecting unsigned integer, got \"modified\""
    );
    assert_eq!(
        t_err("1.2.3-abc-g8c847ab"),
        "expecting unsigned integer, got \"abc\""
    );
    assert_eq!(
        t_err("1.2.3-7-8c847ab"),
        "expecting `g...`, got \"8c847ab\" in \"1.2.3-7-8c847ab\""
    );
}

/// Represent a "standard" version number, using `major.minor.patch`
/// with patch and minor being optional, ignoring an optional `v`
/// prefix on parsing.
#[derive(Debug, PartialEq, Clone)]
pub struct SemVersion(Vec<u32>);

impl SemVersion {
    pub fn next_major(&self) -> Self {
        Self(vec![
            self.0.get(0).copied().expect("major always present") + 1,
        ])
    }

    pub fn next_minor(&self) -> Self {
        let mut ns: Vec<u32> = self.0[0..1].iter().copied().collect();
        ns.push(self.0.get(1).copied().unwrap_or(0) + 1);
        Self(ns)
    }

    pub fn next_patch(&self) -> Self {
        let mut ns: Vec<u32> = self.0.iter().take(2).copied().collect();
        if ns.len() < 2 {
            ns.push(0)
        }
        ns.push(self.0.get(2).copied().unwrap_or(0) + 1);
        Self(ns)
    }
}

#[cfg(test)]
#[test]
fn t_semversion_increment() {
    let p = |s: &str| SemVersion::from_str(s).unwrap();
    assert_eq!(p("0").next_major(), p("1"));
    assert_eq!(p("1").next_major(), p("2"));
    assert_eq!(p("0.1").next_major(), p("1"));
    assert_eq!(p("0.1.3").next_major(), p("1"));
    assert_eq!(p("0.1.3").next_minor(), p("0.2"));
    assert_eq!(p("2.1.3").next_minor(), p("2.2"));
    assert_eq!(p("2.1.3").next_patch(), p("2.1.4"));
    assert_eq!(p("2.1.0").next_patch(), p("2.1.1"));
    assert_eq!(p("2.1").next_patch(), p("2.1.1"));
    assert_eq!(p("2").next_patch(), p("2.0.1"));
}

impl TryFrom<Vec<u32>> for SemVersion {
    type Error = &'static str;

    fn try_from(value: Vec<u32>) -> Result<Self, Self::Error> {
        if value.is_empty() {
            return Err("need a vector with at least one element to make a SemVersion");
        }
        Ok(Self(value))
    }
}

impl Display for SemVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut need_dot = false;
        for val in &self.0 {
            if need_dot {
                f.write_char('.')?;
            }
            Display::fmt(val, f)?;
            need_dot = true;
        }
        Ok(())
    }
}

impl FromStr for SemVersion {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let versionstring = if s.starts_with("v") { &s[1..] } else { s };
        let mut parts: Vec<u32> = Vec::new();
        for (i, part) in versionstring.split('.').enumerate() {
            let n = part.parse().with_context(|| {
                anyhow!(
                    "expecting part {} of the version string {s:?} to be \
                     an integer: {part:?}",
                    i + 1
                )
            })?;
            parts.push(n);
        }
        Ok(Self(parts))
    }
}

/// Does not check whether the first position is 0; i.e. purely
/// calculates the ordering, not semver compatibility.
fn cmp_slices(left: &[u32], right: &[u32]) -> Option<Ordering> {
    fn non_zeroes_mean_less(right: &[u32]) -> Ordering {
        for val in right {
            if *val != 0 {
                return Ordering::Less;
            }
        }
        Ordering::Equal
    }
    let mut i = 0;
    loop {
        if let Some(val_left) = left.get(i) {
            if let Some(val_right) = right.get(i) {
                let cmp = val_left.partial_cmp(val_right)?;
                if cmp != Ordering::Equal {
                    return Some(cmp);
                }
                i += 1;
            } else {
                return Some(non_zeroes_mean_less(&left[i..]).reverse());
            }
        } else {
            return Some(non_zeroes_mean_less(&right[i..]));
        }
    }
}

impl PartialOrd for SemVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        cmp_slices(&self.0, &other.0)
    }
}

/// Compare two values that are optional, treating a missing value as
/// `SemVerOrdResult::Undecidable`. (Somewhat of a misuse of
/// `SemVerOrdResult`, `SemVerOrdResult::Equivalent` always contains
/// `Equal` for no benefit.)
fn cmp_optionals<T: PartialOrd + Debug>(left: Option<T>, right: Option<T>) -> SemVerOrdResult {
    match (left, right) {
        (Some(left), Some(right)) => match left.partial_cmp(&right) {
            Some(ord) => {
                // assume that we are called to compare the level
                // relevant for such decisions
                match ord {
                    Ordering::Equal => SemVerOrdResult::Equivalent(Ordering::Equal),
                    _ => SemVerOrdResult::Upgrade(ord),
                }
            }
            None => SemVerOrdResult::FailedPartialOrd(format!("{left:?} <=> {right:?}").into()),
        },
        (Some(_), None) => {
            SemVerOrdResult::Undecidable(UndecidabilityReason::RightMissing, Ordering::Greater)
        }
        (None, Some(_)) => {
            SemVerOrdResult::Undecidable(UndecidabilityReason::LeftMissing, Ordering::Less)
        }
        (None, None) => {
            SemVerOrdResult::Undecidable(UndecidabilityReason::BothMissing, Ordering::Equal)
        }
    }
}

/// Decides whether 2 version(parts) are compatible. Only works
/// correctly if not both slices have 0 in the first position (because
/// that case would be special)!
fn non0_semver_cmp(left: &[u32], right: &[u32]) -> SemVerOrdResult {
    let (l0, r0) = (left.get(0), right.get(0));
    let cmp0 = cmp_optionals(l0, r0);
    match cmp0 {
        SemVerOrdResult::Equivalent(_ord) => {
            // But return the actual ordering inside, not just the
            // `Equal` that _ord is.
            if let Some(ord) = cmp_slices(left, right) {
                SemVerOrdResult::Equivalent(ord)
            } else {
                SemVerOrdResult::FailedPartialOrd(format!("{left:?} <=> {right:?}").into())
            }
        }
        _ => cmp0,
    }
}

impl SemVerOrd for SemVersion {
    fn semver_cmp(&self, other: &Self) -> SemVerOrdResult {
        let (l0, r0) = (self.0.get(0), other.0.get(0));
        match (l0, r0) {
            (Some(0), Some(0)) => non0_semver_cmp(&self.0[1..], &other.0[1..]),
            _ => non0_semver_cmp(&self.0, &other.0),
        }
    }
}

#[cfg(test)]
#[test]
fn t_version() {
    let t = |s: &str| SemVersion::from_str(s).unwrap();
    let t_err = |s: &str| SemVersion::from_str(s).err().unwrap().to_string();
    assert_eq!(t("2.3.4").0, [2, 3, 4]);
    assert_eq!(t("v2").0, [2]);
    assert_eq!(
        t_err("w2"),
        "expecting part 1 of the version string \"w2\" to be an integer: \"w2\""
    );
    assert_eq!(
        t_err("2.4r5"),
        "expecting part 2 of the version string \"2.4r5\" to be an integer: \"4r5\""
    );
    assert!(t("2.3.4") == t("2.3.4"));
    assert!(t("2.3.5") > t("2.3.4"));
    assert!(t("2.4.5") > t("2.3.4"));
    assert!(t("2.2.5") < t("2.3.4"));
    assert!(t("3.2.5") > t("2.3.4"));
    assert!(t("3.2") < t("3.2.1"));
    assert!(t("3.2.2") > t("3.2"));
    assert!(t("3.1234") > t("3.2"));

    // The left version is not smaller, nor larger, nor the same.
    assert!(!(t("3.2") < t("3.2.0")));
    assert!(!(t("3.2") > t("3.2.0")));
    assert!(t("3.2") != t("3.2.0"));
    // Not equivalent yet: still equal! Uhm. The above uses PartialEq,
    // the following PartialCmp.
    assert_eq!(t("3.2").partial_cmp(&t("3.2.0")).unwrap(), Ordering::Equal);
}

#[cfg(test)]
#[test]
fn t_version_semver_ord() {
    use Ordering::*;
    use SemVerOrdResult::*;
    let t = |s: &str| SemVersion::from_str(s).unwrap();
    assert_eq!(t("1").semver_cmp(&t("1")), Equivalent(Equal));
    assert_eq!(t("1").semver_cmp(&t("2")), Upgrade(Less));
    assert_eq!(t("2").semver_cmp(&t("1")), Upgrade(Greater));
    assert_eq!(t("1.1").semver_cmp(&t("1")), Equivalent(Greater));
    assert_eq!(t("1.2").semver_cmp(&t("2.1")), Upgrade(Less));
    assert_eq!(t("0.2").semver_cmp(&t("1.1")), Upgrade(Less));
    assert_eq!(t("0.1").semver_cmp(&t("0.1")), Equivalent(Equal));
    assert_eq!(t("0.2").semver_cmp(&t("0.1")), Upgrade(Greater));
    assert_eq!(t("0.1.2").semver_cmp(&t("0.1")), Equivalent(Greater));
    assert_eq!(t("0.1.2").semver_cmp(&t("0.1.9")), Equivalent(Less));
    assert_eq!(t("0.1.2").semver_cmp(&t("0.1.2")), Equivalent(Equal));
    assert_eq!(t("0.1.2.0.1").semver_cmp(&t("0.1.2")), Equivalent(Greater));
    assert_eq!(
        t("0.1").semver_cmp(&t("0")),
        Undecidable(UndecidabilityReason::RightMissing, Greater)
    );
}

impl PartialOrd for GitVersion<SemVersion> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let cmp = self.version.partial_cmp(&other.version)?;
        if cmp != Ordering::Equal {
            return Some(cmp);
        }
        // Vague/problematic comparison business!: assumes that higher
        // depth means higher "version".
        if let Some((depth_left, _sha1_left)) = &self.past_tag {
            if let Some((depth_right, _sha1_right)) = &other.past_tag {
                if self.version == other.version {
                    depth_left.partial_cmp(&depth_right)
                } else {
                    // XX proper logging
                    // eprintln!(
                    //     "equivalent but not identical version tags: {:?} vs. {:?}",
                    //     self.version, other.version
                    // );
                    None
                }
            } else {
                Some(Ordering::Greater)
            }
        } else {
            if let Some((_depth_right, _sha1_right)) = &other.past_tag {
                Some(Ordering::Less)
            } else {
                Some(Ordering::Equal)
            }
        }
    }
}

#[cfg(test)]
#[test]
fn t_git_version_version() {
    let t = |s: &str| -> GitVersion<SemVersion> { s.parse().unwrap() };
    assert_eq!(
        t("1.2.3-7-g8c847ab"),
        GitVersion {
            version: "1.2.3".parse().unwrap(),
            past_tag: Some((7, "8c847ab".into())),
            modified: false
        }
    );
    // Copy-paste of the tests for `Version`
    assert!(t("2.3.4") == t("2.3.4"));
    assert!(t("2.3.5") > t("2.3.4"));
    assert!(t("2.4.5") > t("2.3.4"));
    assert!(t("2.2.5") < t("2.3.4"));
    assert!(t("3.2.5") > t("2.3.4"));
    assert!(t("3.2") < t("3.2.1"));
    assert!(t("3.2.2") > t("3.2"));
    assert!(t("3.1234") > t("3.2"));
    assert!(!(t("3.2") < t("3.2.0")));
    assert!(!(t("3.2") > t("3.2.0")));
    assert!(t("3.2") != t("3.2.0"));
    assert_eq!(t("3.2").partial_cmp(&t("3.2.0")).unwrap(), Ordering::Equal);

    // For the whole thing:
    assert!(t("2.3.5-4-gab8e32") > t("v2.3.4-4-gab8e32"));
    // XX oh, checking for identical tags above but it misses the "v" prefix change!
    assert!(t("2.3.4-4-gab8e32") == t("v2.3.4-4-gab8e32"));
    assert!(t("2.3.4-5-gab8e32") > t("v2.3.4-4-gab8e32"));
    assert!(t("2.3.4-3-gab8e32") < t("v2.3.4-4-gab8e32"));
    assert!(t("2.3.4") < t("v2.3.4-4-gab8e32"));
    assert!(t("2.3.4-1-gab1234") > t("v2.3.4"));
    assert!(t("2.3.4-1-gab1234") < t("v2.3.5"));

    assert!(t("2.3.0-3-gab8e32") != t("2.3-3-gab8e32"));
    assert!(!(t("2.3.0-3-gab8e32") < t("2.3-3-gab8e32")));
    assert!(!(t("2.3.0-3-gab8e32") > t("2.3-3-gab8e32")));
    assert_eq!(t("2.3.0-3-gab8e32").partial_cmp(&t("2.3-3-gab8e32")), None);

    assert!(t("2.3.0-3-gab8e32") != t("2.3.0-3-g123456"));
    assert_eq!(
        t("2.3.0-3-gab8e32").partial_cmp(&t("2.3.0-3-g123456")),
        Some(Ordering::Equal)
    );
}

impl SemVerOrd for GitVersion<SemVersion> {
    fn semver_cmp(&self, other: &Self) -> SemVerOrdResult {
        let paranoid = false;

        // Work in progress means, pessimistically, breaking
        // changes. Ordering still as per PartialOrd, but
        // upgradability in doubt--even in any case, not only to the
        // next-higher version?
        let wips = (self.past_tag.is_some(), other.past_tag.is_some());
        let any_wip = wips.0 | wips.1;

        // Use this as ordering in SemVerOrdResult? If it is given,
        // anyway.
        let ord_partialord = self.partial_cmp(&other);

        let cmp = self.version.semver_cmp(&other.version);
        match cmp {
            SemVerOrdResult::Equivalent(ord) => {
                if any_wip {
                    SemVerOrdResult::Undecidable(
                        UndecidabilityReason::Wip(wips.0, wips.1),
                        ord_partialord.unwrap_or(ord),
                    )
                } else {
                    cmp
                }
            }
            SemVerOrdResult::Upgrade(ord) => {
                if paranoid && any_wip {
                    SemVerOrdResult::Undecidable(
                        UndecidabilityReason::Wip(wips.0, wips.1),
                        ord_partialord.unwrap_or(ord),
                    )
                } else {
                    cmp
                }
            }
            SemVerOrdResult::Undecidable(_, _) => cmp,
            SemVerOrdResult::FailedPartialOrd(_) => cmp,
        }
    }
}

#[cfg(test)]
#[test]
fn t_git_version_semver_ord() {
    let t = |left: &str, right: &str| -> SemVerOrdResult {
        let left: GitVersion<SemVersion> = left.parse().unwrap();
        let right: GitVersion<SemVersion> = right.parse().unwrap();
        left.semver_cmp(&right)
    };
    use Ordering::*;
    use SemVerOrdResult::*;
    use UndecidabilityReason::*;
    assert_eq!(t("2.3.4", "2.3.4"), Equivalent(Equal));
    assert_eq!(t("2.3.5", "2.3.4"), Equivalent(Greater));
    assert_eq!(t("2.5", "2.3.4"), Equivalent(Greater));
    assert_eq!(t("3.5", "2.3.4"), Upgrade(Greater));
    assert_eq!(t("3", "2.3.4"), Upgrade(Greater));
    assert_eq!(t("0.3", "2.3.4"), Upgrade(Less));
    assert_eq!(t("0.3", "0.4"), Upgrade(Less));
    assert_eq!(t("0.3.9", "0.4"), Upgrade(Less));

    assert_eq!(
        t("0.3.9-4-gab1234", "0.3"),
        Undecidable(Wip(true, false), Greater)
    );
    assert_eq!(
        t("0.3.9-4-gab1234", "0.3.9"),
        Undecidable(Wip(true, false), Greater)
    );
    assert_eq!(
        t("0.3.9-4-gab1234", "0.3.10"),
        Undecidable(Wip(true, false), Less)
    );
    assert_eq!(
        t("0.0", "0.3.9-4-gab1234"),
        // paranoid: Undecidable(Wip(false, true), Less)
        Upgrade(Less)
    );
    assert_eq!(
        t("0.3.9-4-gab1234", "0.0"),
        // paranoid: Undecidable(Wip(true, false), Greater)
        Upgrade(Greater)
    );
    assert_eq!(t("0", "0.3.9-4-gab1234"), Undecidable(LeftMissing, Less));
    // Is it OK to put wip versions *inbetween* their base version and
    // the next semver version? Or should wip mean that work can be
    // the next semver version and hence conflict and yeald none?
    // Well, now returns undecidable anyway. -- Well, not anymore
    // (will really need a paranoia setting).
    assert_eq!(
        t("0.3.9-4-gab1234", "0.4"),
        // paranoid: Undecidable(Wip(true, false), Less)
        Upgrade(Less)
    );
}
