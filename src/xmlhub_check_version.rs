// Part of xmlhub, this is the first file after
// xmlhub_indexer_defaults.rs that shall remain part of the lib crate
// of the xmlhub + make-release binaries.

//! Wrapper around git_check_version.rs for the xmlhub specific parts.

use std::{borrow::Cow, path::Path};

use anyhow::Result;

use crate::{
    git_check_version::GitLogVersionChecker,
    git_version::{GitVersion, SemVersion},
    ref_or_owned::RefOrOwned,
    xmlhub_types::OutputFile,
};

pub struct XmlhubCheckVersion<'s> {
    pub program_name: &'s str,
    pub program_version: RefOrOwned<'s, GitVersion<SemVersion>>,
    pub no_version_check: bool,
    pub base_path: Cow<'s, Path>,
    pub html_file: RefOrOwned<'s, OutputFile>,
    pub md_file: RefOrOwned<'s, OutputFile>,
}

impl<'s> XmlhubCheckVersion<'s> {
    fn git_log_version_checker(&self) -> GitLogVersionChecker {
        GitLogVersionChecker {
            program_name: self.program_name.into(),
            program_version: self.program_version.as_ref().into(),
        }
    }

    /// Verify that this is not an outdated version of the program.
    pub fn check_git_log(&self) -> Result<()> {
        if !self.no_version_check {
            let git_log_version_checker = self.git_log_version_checker();
            let program_name = self.program_name;

            let found = git_log_version_checker.check_git_log(
                self.base_path.as_ref(),
                &[
                    self.html_file.path_from_repo_top,
                    self.md_file.path_from_repo_top,
                ],
                Some(format!(
                    "please upgrade your copy of the {program_name} program with:\n  \
                     `{program_name} upgrade`\n\
                     If you know what you are doing and are sure that you want to proceed \
                     anyway, use the `--no-version-check` option."
                )),
            )?;
            if found.is_none() {
                println!(
                    "Warning: could not find or parse {program_name} version statements \
                     in the git log on the output files; this may mean that \
                     this is a fresh xmlhub Git repository, or something is messed up. \
                     This means that if {program_name} is used from another computer, \
                     if its version is producing different output from this version \
                     then each will overwrite the changes from the other endlessly."
                );
            }
        }
        Ok(())
    }

    /// Delegate to `GitLogVersionChecker`
    pub fn program_name_and_version(&self) -> String {
        self.git_log_version_checker().program_name_and_version()
    }
}
