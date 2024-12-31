use std::{cmp::Ordering, ffi::OsStr, fmt::Debug, path::Path};

use thiserror::Error;

use crate::{
    git::{git_log, GitLogEntry},
    git_version::{GitVersion, SemVerOrd, SemVerOrdResult, SemVersion, UndecidabilityReason},
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

    #[error("checking for program version numbers in the git log")]
    OtherError(#[from] anyhow::Error),
}

fn check_version(
    program_version: &GitVersion<SemVersion>,
    data_version: &GitVersion<SemVersion>,
) -> Result<Option<Ordering>, GitCheckVersionError> {
    macro_rules! decide_on_ord {
        {$ord:ident, {$($reason:tt)*}, $($err_constructor:tt)*} => {
            match $ord {
                Ordering::Greater | Ordering::Equal => Ok(Some($ord)),
                Ordering::Less => Err($($err_constructor)* {
                    $($reason)*
                    program_version: program_version.clone(),
                    data_version: data_version.clone()
                }),
            }
        }
    }

    match program_version.semver_cmp(&data_version) {
        // XX so, equivalent only happens when there is no wip right?
        SemVerOrdResult::Equivalent(ord) => return Ok(Some(ord)),
        SemVerOrdResult::Upgrade(ord) => {
            return decide_on_ord!(ord, {}, GitCheckVersionError::ProgramTooOld)
        }
        // XX so, we can't reconstruct if semantic version was same ?
        SemVerOrdResult::Undecidable(reason, ord) => {
            let ok = || Ok(Some(ord));
            let potentially_too_old = || {
                decide_on_ord!(ord, {reason: reason,},
                                  GitCheckVersionError::ProgramPotentiallyTooOld)
            };
            return match reason {
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
            };
        }
        SemVerOrdResult::FailedPartialOrd(vals_string) => {
            return Err(GitCheckVersionError::CouldNotCompare {
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
    assert_eq!(t("0.2.3", "0.2.3"), Some(Equal));

    assert_eq!(t("0.2.3", "0.1"), Some(Greater));
    assert_eq!(t("0.2.3", "0.2.2"), Some(Greater));
    assert_eq!(t("0.2.2", "0.2.3"), Some(Less));
    assert_eq!(t("2", "0.2"), Some(Greater));
    assert_eq!(t("2", "1"), Some(Greater));
    assert_eq!(t("2.2", "2.3"), Some(Less));
    assert_eq!(t("2.3", "2.2"), Some(Greater));
    assert_eq!(t("1.1", "1.2.3"), Some(Less));
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
    assert_eq!(t("0.2.3-5-g2343", "0.2.3-4-g18881"), Some(Greater));
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
    assert_eq!(t("0.2.4", "0.2.3-5-g2343"), Some(Greater));
    assert_eq!(t("0.3", "0.2.3-5-g2343"), Some(Greater));
    assert_eq!(t("0.2.3-5-g2343", "0.2.3"), Some(Greater));
    // Is this OK? If people don't work on multiple parallel versions,
    // then "1.5" is presumably releaste after that wip on "1", and
    // hence incorporates the work, and is hence OK.
    assert_eq!(t("1.5", "1-5-g1234"), Some(Greater));
    // Yes that's ok, definitely released new version should take care
    // of these changes; sure, not guaranteed with parallel version
    // branches, but good enough:
    assert_eq!(t("2", "1-5-g1234"), Some(Greater));
}

/// Check a Git log for written-down version numbers, when found, do a
/// SemVer comparison with the given version, if our_version is less
/// than the version found, report an error. Returns the ordering
/// comparison from the program version to the found version, which
/// might be `Less`, if both versions are still semver
/// compatible. Returns None if nothing was found.
pub fn git_check_version<S: AsRef<OsStr> + Debug>(
    base_path: &Path,
    git_log_arguments: &[S],
    parse: impl Fn(&GitLogEntry) -> Option<GitVersion<SemVersion>>,
    program_version: &GitVersion<SemVersion>,
) -> Result<Option<Ordering>, GitCheckVersionError> {
    for entry in git_log(base_path, git_log_arguments)? {
        let entry = entry?;
        if let Some(found_version) = parse(&entry) {
            return check_version(program_version, &found_version);
        }
    }
    Ok(None)
}
