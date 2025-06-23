use std::{
    borrow::Cow,
    cmp::Ordering,
    ffi::OsStr,
    fmt::Debug,
    path::{Path, PathBuf},
};

use thiserror::Error;

use crate::{
    git::GitWorkingDir,
    git_version::{GitVersion, SemVerOrd, SemVerOrdResult, SemVersion, UndecidabilityReason},
    ref_or_owned::RefOrOwned,
};

// fn wip_format(program_version_is_wip: bool, data_version_is_wip: bool) -> &'static str {
//     match (program_version_is_wip, data_version_is_wip) {
//         (true, true) => "both the program and data versions",
//         (true, false) => "the program version",
//         (false, true) => "the data version",
//         (false, false) => unreachable!(),
//     }
// }

fn potentially_too_old_format(
    program_version: &GitVersion<SemVersion>,
    data_version: &GitVersion<SemVersion>,
    reason: &UndecidabilityReason,
) -> String {
    match reason {
        UndecidabilityReason::Wip(prog_wip, data_wip) => match (prog_wip, data_wip) {
            (true, true) => format!(
                "both the version of the running program ({program_version}) and \
                 the program that produced the data ({data_version}) are unreleased \
                 versions in the same SemVer range"
            ),
            (true, false) => format!(
                "the version of the running program ({program_version}) is \
                 below the version of the program that produced the data \
                 ({data_version}), and even though in the same SemVer range, \
                 the program is an unreleased version, and even if released \
                 as {data_version} later, it may have had breaking changes inbetween"
            ),
            (false, true) => format!(
                "the version of the running program ({program_version}) is \
                 below the version of the program that produced the data \
                 ({data_version}, which is also an unreleased version), hence \
                 likely too old"
            ),
            (false, false) => unreachable!(),
        },
        UndecidabilityReason::LeftMissing
        | UndecidabilityReason::RightMissing
        | UndecidabilityReason::BothMissing => format!(
            "can't evaluate program version due to invalid version numbers \
             ({reason})"
        ),
    }
}

#[derive(Debug, Error)]
pub enum GitCheckVersionError {
    #[error(
        "this program's version ({program_version}) is too old: it is \
         below the version of the program that produced the existing \
         output ({data_version}) and outside of the shared SemVer-compatible range"
    )]
    ProgramTooOld {
        program_version: GitVersion<SemVersion>,
        data_version: GitVersion<SemVersion>,
    },

    #[error(
        "this program is potentially too old: {}",
        potentially_too_old_format(program_version, data_version, reason)
    )]
    ProgramPotentiallyTooOld {
        program_version: GitVersion<SemVersion>,
        data_version: GitVersion<SemVersion>,
        reason: UndecidabilityReason,
    },

    #[error(
        "error: could not compare the program version {program_version} with the \
         version of the data, {data_version}: {message}"
    )]
    CouldNotCompare {
        message: Box<String>,
        program_version: GitVersion<SemVersion>,
        data_version: GitVersion<SemVersion>,
    },

    #[error("error {0:#}")]
    OtherError(#[from] anyhow::Error),
}

impl GitCheckVersionError {
    pub fn is_version_error(&self) -> bool {
        match self {
            GitCheckVersionError::ProgramTooOld {
                program_version: _,
                data_version: _,
            } => true,
            GitCheckVersionError::ProgramPotentiallyTooOld {
                program_version: _,
                data_version: _,
                reason: _,
            } => true,
            GitCheckVersionError::CouldNotCompare {
                message: _,
                program_version: _,
                data_version: _,
            } => false,
            GitCheckVersionError::OtherError(_) => false,
        }
    }
    pub fn extend(
        self,
        base_path: &Path,
        what_to_do: Option<String>,
    ) -> GitCheckVersionErrorWithContext {
        GitCheckVersionErrorWithContext {
            base_path: base_path.to_owned(),
            error: self,
            what_to_do,
        }
    }
}

fn check_version(
    program_version: &GitVersion<SemVersion>,
    data_version: &GitVersion<SemVersion>,
) -> Result<Ordering, GitCheckVersionError> {
    macro_rules! decide_on_ord {
        {$ord:ident, {$($reason:tt)*}, $($err_constructor:tt)*} => {
            match $ord {
                Ordering::Greater | Ordering::Equal => Ok($ord),
                Ordering::Less => Err($($err_constructor)* {
                    $($reason)*
                    program_version: program_version.clone(),
                    data_version: data_version.clone()
                }),
            }
        }
    }

    match program_version.semver_cmp(data_version) {
        // XX so, equivalent only happens when there is no wip right?
        SemVerOrdResult::Equivalent(ord) => Ok(ord),
        SemVerOrdResult::Upgrade(ord) => {
            decide_on_ord!(ord, {}, GitCheckVersionError::ProgramTooOld)
        }
        // XX so, we can't reconstruct if semantic version was same ?
        SemVerOrdResult::Undecidable(reason, ord) => {
            let ok = || Ok(ord);
            let potentially_too_old = || {
                decide_on_ord!(
                    ord,
                    { reason: reason, },
                    GitCheckVersionError::ProgramPotentiallyTooOld
                )
            };
            match reason {
                UndecidabilityReason::Wip(prog_is_wip, data_is_wip) => {
                    match (prog_is_wip, data_is_wip, ord) {
                        (true, false, Ordering::Less) => potentially_too_old(),
                        (true, false, _) => ok(),
                        (true, true, _) => potentially_too_old(),
                        (false, _, Ordering::Greater) => ok(), // XX?
                        (false, _, _) => potentially_too_old(),
                    }
                }
                _ => potentially_too_old(),
            }
        }
        SemVerOrdResult::FailedPartialOrd(vals_string) => {
            Err(GitCheckVersionError::CouldNotCompare {
                message: vals_string,
                program_version: program_version.clone(),
                data_version: data_version.clone(),
            })
        }
    }
}

#[cfg(test)]
#[test]
fn t_check_version() {
    let t = |program: &str, data: &str| {
        let program: GitVersion<SemVersion> = program.parse().unwrap();
        let data: GitVersion<SemVersion> = data.parse().unwrap();
        check_version(&program, &data).unwrap()
    };
    let t_err = |program: &str, data: &str| {
        let program: GitVersion<SemVersion> = program.parse().unwrap();
        let data: GitVersion<SemVersion> = data.parse().unwrap();
        check_version(&program, &data).err().unwrap().to_string()
    };
    use Ordering::*;
    assert_eq!(t("0.2.3", "0.2.3"), Equal);

    assert_eq!(t("0.2.3", "0.1"), Greater);
    assert_eq!(t("0.2.3", "0.2.2"), Greater);
    assert_eq!(t("0.2.2", "0.2.3"), Less);
    assert_eq!(t("2", "0.2"), Greater);
    assert_eq!(t("2", "1"), Greater);
    assert_eq!(t("2.2", "2.3"), Less);
    assert_eq!(t("2.3", "2.2"), Greater);
    assert_eq!(t("1.1", "1.2.3"), Less);
    assert_eq!(
        t_err("0.1", "0.2.3"),
        "this program's version (0.1) is too old: it is below the version of the program that produced the existing output (0.2.3) and outside of the shared SemVer-compatible range"
    );
    assert_eq!(
        t_err("0.2-5-g2343", "0.2.3"),
        "this program is potentially too old: the version of the running program (0.2-5-g2343) is below the version of the program that produced the data (0.2.3), and even though in the same SemVer range, the program is an unreleased version, and even if released as 0.2.3 later, it may have had breaking changes inbetween"
    );
    assert_eq!(
        t_err("0.2.3", "0.2.3-5-g2343"),
        "this program is potentially too old: the version of the running program (0.2.3) is below the version of the program that produced the data (0.2.3-5-g2343, which is also an unreleased version), hence likely too old"
    );
    assert_eq!(t("0.2.3-5-g2343", "0.2.3-4-g18881"), Greater);
    assert_eq!(
        t_err("0.2.2-5-g2343", "0.2.3-4-g18881"),
        "this program is potentially too old: both the version of the running program (0.2.2-5-g2343) and the program that produced the data (0.2.3-4-g18881) are unreleased versions in the same SemVer range"
    );
    assert_eq!(
        t_err("0.2.2-5-g2343", "2-4-g18881"),
        "this program's version (0.2.2-5-g2343) is too old: it is below the version of the program that produced the existing output (2-4-g18881) and outside of the shared SemVer-compatible range"
    );
    assert_eq!(
        t_err("0.2.2-5-g2343", "1.2-4-g18881"),
        "this program's version (0.2.2-5-g2343) is too old: it is below the version of the program that produced the existing output (1.2-4-g18881) and outside of the shared SemVer-compatible range"
    );
    // Why is this OK? See below.
    assert_eq!(t("0.2.4", "0.2.3-5-g2343"), Greater);
    assert_eq!(t("0.3", "0.2.3-5-g2343"), Greater);
    assert_eq!(t("0.2.3-5-g2343", "0.2.3"), Greater);
    // Is this OK? If people don't work on multiple parallel versions,
    // then "1.5" is presumably releaste after that wip on "1", and
    // hence incorporates the work, and is hence OK.
    assert_eq!(t("1.5", "1-5-g1234"), Greater);
    // Yes that's ok, definitely released new version should take care
    // of these changes; sure, not guaranteed with parallel version
    // branches, but good enough:
    assert_eq!(t("2", "1-5-g1234"), Greater);
}

#[derive(Debug, Error)]
#[error("{}checking the git log at {base_path:?}: {error}{}",
        if let Some(what) = what_to_do {
            format!("{what}\n(While ")
        } else {
            "".into()
        },
        if what_to_do.is_some() { ")" } else { "" }
)]
pub struct GitCheckVersionErrorWithContext {
    pub base_path: PathBuf,
    pub error: GitCheckVersionError,
    pub what_to_do: Option<String>,
}

pub struct GitLogVersionChecker<'t> {
    pub program_name: Cow<'t, str>,
    pub program_version: RefOrOwned<'t, GitVersion<SemVersion>>,
}

impl<'t> GitLogVersionChecker<'t> {
    /// Give program name and version split over 3 lines, in a format
    /// that can be parsed back by `parse_version_from_message` /
    /// `check_git_log`.
    pub fn program_name_and_version(&self) -> String {
        format!(
            "{}\n\nversion: {}",
            self.program_name, *self.program_version
        )
    }

    pub fn parse_version_from_message(&self, message: &str) -> Option<GitVersion<SemVersion>> {
        let mut lines = message.split('\n');
        while let Some(line) = lines.next() {
            if line.contains(self.program_name.as_ref()) {
                // Loop for the version number in the next 2 lines.
                for line in lines.clone().take(2) {
                    let body_key = "version:";
                    if line.starts_with(body_key) {
                        let version_str = line[body_key.as_bytes().len()..].trim();
                        if let Ok(version) = version_str.parse() {
                            return Some(version);
                        }
                    }
                }
                // Otherwise backtrack and continue.
            }
        }
        None
    }

    /// Check a Git log for written-down version numbers, when found,
    /// do a SemVer comparison with the given version, if the
    /// `program_name` is less than the version found, report an
    /// error. Returns the ordering comparison from the program
    /// version to the found version, which might be `Less`, if both
    /// versions are still semver compatible, and the found version,
    /// or `None` if nothing was found. `what_to_do` is made part of
    /// the error if it is because of an insufficient version issue;
    /// it is ignored if the error is due to something else.
    pub fn check_git_log<S: AsRef<OsStr> + Debug>(
        &self,
        git_working_dir: &GitWorkingDir,
        git_log_arguments: &[S],
        what_to_do: Option<String>,
    ) -> Result<Option<(Ordering, GitVersion<SemVersion>)>, GitCheckVersionErrorWithContext> {
        (|| {
            for entry in git_working_dir.git_log(git_log_arguments)? {
                let entry = entry?;
                if let Some(found_version) = self.parse_version_from_message(&entry.message) {
                    let ordering = check_version(&self.program_version, &found_version)?;
                    return Ok(Some((ordering, found_version)));
                }
            }
            Ok(None)
        })()
        .map_err(|e: GitCheckVersionError| {
            let what_to_do = if e.is_version_error() {
                what_to_do
            } else {
                None
            };
            e.extend(git_working_dir.working_dir_path_ref(), what_to_do)
        })
    }
}
