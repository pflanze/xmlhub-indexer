use std::{
    borrow::Cow,
    ffi::OsString,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use anyhow::{anyhow, bail, Result};
use nix::NixPath;

use crate::{
    const_util::file_name, fixup_path::FixupPath, git::GitWorkingDir, path_util::AppendToPath,
};

#[derive(Debug, Clone)]
pub struct CheckoutContext<'s, P: AsRef<Path>> {
    /// The path from the current directory of the process to the
    /// working directory of the checkout. Could be of type &Path or
    /// &str (or anything else that can be referenced as &Path).
    pub working_dir_path: P,
    /// The name of the branch used in the remote repository, also
    /// expected to be the same in the local checkout
    pub branch_name: &'s str,
    /// Where the upstream repository should be, for `git clone` (and
    /// push) purposes.
    pub supposed_upstream_git_url: &'s str,
    /// Where the upstream repository should be, for visiting by web
    /// browser. Note that for `supposed_upstream_repo_name()` to
    /// work, this must end in the repository name without a slash at
    /// the end.
    pub supposed_upstream_web_url: &'s str,
    /// Check these sub-paths in a repository when
    /// `CheckExpectedSubpathsExist::Yes` is given to `check1`
    pub expected_sub_paths: &'s [&'s str],
}

/// Whether to check that the expected sub-paths (as per
/// `CheckoutContext::expected_sub_paths`) exist in a repository
#[derive(Debug, Clone, Copy)]
pub enum CheckExpectedSubpathsExist {
    Yes,
    No,
}

impl<'s, P: AsRef<Path>> CheckoutContext<'s, P> {
    pub fn working_dir_path(&self) -> &Path {
        self.working_dir_path.as_ref()
    }

    pub fn git_working_dir(&self) -> GitWorkingDir {
        GitWorkingDir::from(self.working_dir_path().to_owned())
    }

    /// Does not check `path`. Call `check1` later if you like.
    pub fn replace_working_dir_path<'t, 'p, P2>(&'t self, path: P2) -> CheckoutContext<'s, P2>
    where
        's: 'p,
        P: Clone,
        P2: AsRef<Path> + 'p, // ?
    {
        // Can't use the `.. self.clone()` struct update syntax
        // because self and the result value have different type
        // parameters. Destruct and reconstruct explicitly/fully
        // instead:
        let CheckoutContext {
            working_dir_path: _,
            branch_name,
            supposed_upstream_git_url,
            supposed_upstream_web_url,
            expected_sub_paths,
        } = self.clone();
        CheckoutContext {
            working_dir_path: path,
            branch_name,
            supposed_upstream_git_url,
            supposed_upstream_web_url,
            expected_sub_paths,
        }
    }

    /// The name of a repository in upstream (i.e. what `git clone
    /// $url` chooses as the local directory name for the clone);
    /// relies on the web url ending with that name.
    pub fn supposed_upstream_repo_name(&self) -> &'s str {
        file_name(&self.supposed_upstream_web_url)
    }

    /// Accepts any subpath inside this repository and finds the
    /// correct `working_dir_path` by checking the subpath and its
    /// parents until it finds this repository; if it finds it and it
    /// checks out OK, returns it, otherwise an error. Note:
    /// canonicalizes `path` before doing its work, and the result
    /// contains it or part of it rather than the original path or
    /// part thereof. If `allow_subrepositories` is true, will
    /// continue looking upwards when it finds a dir with `.git` that
    /// doesn't fit us; otherwise the check1 error is reported at that
    /// point.
    pub fn checked_from_subpath<'p, P2>(
        &'s self,
        path: P2,
        subpath_check: CheckExpectedSubpathsExist,
        allow_subrepositories: bool,
    ) -> Result<CheckedCheckoutContext1<'s, Cow<'s, Path>>>
    where
        's: 'p,
        P: Clone,
        P2: AsRef<Path> + 'p, // ?
    {
        let absolute = path.as_ref().canonicalize()?;
        let mut current_path: &Path = &absolute;
        let mut first_error = None;
        while !current_path.is_empty() {
            // XX is_dir()? How do the shared-database things work?
            if current_path.append(".git").exists() {
                let repo = self.replace_working_dir_path(Cow::from(current_path.to_owned()));
                match repo.check1(subpath_check) {
                    Ok(r) => return Ok(r),
                    Err(e) => {
                        if !allow_subrepositories {
                            return Err(e);
                        } else {
                            if first_error.is_none() {
                                first_error = Some(e)
                            }
                        }
                    }
                }
            }
            if let Some(parent) = current_path.parent() {
                current_path = parent;
            } else {
                break;
            }
        }

        if let Some(first_error) = first_error {
            Err(first_error)
        } else {
            bail!("directory is not inside a Git working directory");
        }
    }

    /// Check that working directory is clean.
    pub fn check_status(&self) -> Result<()> {
        let items = self.git_working_dir().git_status()?;
        if !items.is_empty() {
            bail!(
                "uncommitted changes in the git checkout at {:?}: {items:?}",
                self.working_dir_path()
            );
        }
        Ok(())
    }

    /// Checks that the working dir exists (and has a .git subdir),
    /// and if requested via `subpath_check`, checks for the expected
    /// sub-paths. For further checks, see
    /// `CheckedCheckoutContext1::check2`.
    pub fn check1(
        self,
        subpath_check: CheckExpectedSubpathsExist,
    ) -> Result<CheckedCheckoutContext1<'s, P>> {
        let working_dir_path: &Path = self.working_dir_path.as_ref();
        if !self.checkout_dir_exists() {
            bail!(
                "missing git working directory at the path {:?}; \
                 if you have given the correct path then please run: \
                 `cd {:?}; git clone {} {:?}; cd -`",
                working_dir_path,
                working_dir_path
                    .parent()
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| PathBuf::from("???"))
                    .fixup(),
                self.supposed_upstream_git_url,
                working_dir_path
                    .file_name()
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| OsString::from("???")),
            )
        }
        let git_path = working_dir_path.append(".git");
        // XX use .is_dir()?
        if !git_path.exists() {
            bail!(
                "directory {working_dir_path:?} is not a Git clone (it does not contain \
                 a `.git` subdirectory)"
            )
        }
        match subpath_check {
            CheckExpectedSubpathsExist::Yes => {
                for sub_path_str in self.expected_sub_paths {
                    let sub_path: &Path = sub_path_str.as_ref();
                    let working_dir_path: &Path = self.working_dir_path.as_ref();
                    let path = working_dir_path.append(sub_path);
                    if !path.exists() {
                        bail!(
                            "the directory {working_dir_path:?} does not look like a clone of \
                     {:?}, it is missing the entry {path:?}",
                            self.supposed_upstream_web_url
                        )
                    }
                }
            }
            CheckExpectedSubpathsExist::No => (),
        }
        Ok(CheckedCheckoutContext1 {
            parent: self,
            branch_is_checked: AtomicBool::new(false).into(),
        })
    }

    /// See docs on `CheckedCheckoutContext1::check2`; this just calls
    /// `self.check1()?.check2()`.
    pub fn check2(
        self,
        subpath_check: CheckExpectedSubpathsExist,
    ) -> Result<CheckedCheckoutContext2<'s, P>> {
        self.check1(subpath_check)?.check2()
    }

    fn checkout_dir_exists(&self) -> bool {
        let path: &Path = self.working_dir_path.as_ref();
        path.exists()
    }
}

/// Stores a flag to cache the result of calling
/// `check_current_branch` as an optimization, across clones and
/// threads.
#[derive(Debug, Clone)]
pub struct CheckedCheckoutContext1<'s, P: AsRef<Path>> {
    parent: CheckoutContext<'s, P>,
    branch_is_checked: Arc<AtomicBool>,
}

impl<'s, P: AsRef<Path>> std::ops::Deref for CheckedCheckoutContext1<'s, P> {
    type Target = CheckoutContext<'s, P>;

    fn deref(&self) -> &Self::Target {
        &self.parent
    }
}

impl<'s, P: AsRef<Path>> CheckedCheckoutContext1<'s, P> {
    /// "Hide" check1 from the parent (could just make it the identity
    /// function instead, if you want).
    pub fn check1(self) -> ! {
        unimplemented!()
    }

    /// Checks that the checked-out branch is the specified one
    /// (unless `check_current_branch` was already called), and gets
    /// the default remote for that branch.
    pub fn check2(self) -> Result<CheckedCheckoutContext2<'s, P>> {
        self.check_current_branch()?;
        let default_remote = self.git_remote_get_default()?;
        Ok(CheckedCheckoutContext2 {
            parent: self,
            default_remote,
        })
    }

    /// Check that the checked-out branch is the specified one, unless
    /// successfully called before. That boolean is shared across
    /// clones (including across threads) of the
    /// `CheckedCheckoutContext1` that was returned from `check1`.
    pub fn check_current_branch(&self) -> Result<()> {
        if self.branch_is_checked.load(Ordering::Relaxed) {
            return Ok(());
        }
        let current_branch = self.git_working_dir().git_branch_show_current()?;
        if current_branch.as_deref() != Some(self.branch_name) {
            bail!(
                "expecting checked-out branch to be `{}`, but it is `{}`",
                self.branch_name,
                current_branch
                    .as_deref()
                    .unwrap_or("none, i.e. detached head")
            )
        }
        self.branch_is_checked.store(true, Ordering::Relaxed);
        Ok(())
    }

    /// The remote name, not URL
    fn git_remote_get_default(&self) -> Result<String> {
        self.git_working_dir()
            .git_remote_get_default_for_branch(self.branch_name)?
            .ok_or_else(|| {
                anyhow!(
                    "branch {:?} in {:?} does not have a default remote set",
                    self.branch_name,
                    self.working_dir_path()
                )
            })
    }
}

#[derive(Debug, Clone)]
pub struct CheckedCheckoutContext2<'s, P: AsRef<Path>> {
    parent: CheckedCheckoutContext1<'s, P>,
    /// The remote name, not URL
    pub default_remote: String,
}

impl<'s, P: AsRef<Path>> std::ops::Deref for CheckedCheckoutContext2<'s, P> {
    type Target = CheckoutContext<'s, P>;

    fn deref(&self) -> &Self::Target {
        &self.parent
    }
}

impl<'s, P: AsRef<Path>> CheckedCheckoutContext2<'s, P> {
    /// Returns the full reference name starting with "remotes/".
    pub fn remote_branch_reference(&self) -> String {
        format!("remotes/{}/{}", self.default_remote, self.branch_name)
    }
}
