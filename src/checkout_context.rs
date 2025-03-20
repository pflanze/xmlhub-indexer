use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use anyhow::{anyhow, bail, Result};

use crate::git::{git_branch_show_current, git_remote_get_default_for_branch, git_status};

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
    /// push) purposes. Currently only used for error message.
    pub supposed_upstream_git_url: &'s str,
    /// Where the upstream repository should be, for visiting by web
    /// browser. Ditto.
    pub supposed_upstream_web_url: &'s str,
}

impl<'s, P: AsRef<Path>> CheckoutContext<'s, P> {
    pub fn replace_working_dir_path<'p, P2>(&'s self, path: P2) -> CheckoutContext<'p, P2>
    where
        's: 'p,
        P: Clone,
        P2: AsRef<Path> + 'p, // ?
    {
        let CheckoutContext {
            working_dir_path: _,
            branch_name,
            supposed_upstream_git_url,
            supposed_upstream_web_url,
        } = self.clone();
        CheckoutContext {
            working_dir_path: path,
            branch_name,
            supposed_upstream_git_url,
            supposed_upstream_web_url,
        }
    }

    pub fn check_status(&self) -> Result<()> {
        let items = git_status(self.working_dir_path())?;
        if !items.is_empty() {
            bail!(
                "uncommitted changes in the git checkout at {:?}: {items:?}",
                self.working_dir_path()
            );
        }
        Ok(())
    }

    pub fn working_dir_path(&self) -> &Path {
        self.working_dir_path.as_ref()
    }

    /// Checks that the working dir exists. For further checks, see
    /// `CheckedCheckoutContext1::check2`.
    pub fn check1(self) -> Result<CheckedCheckoutContext1<'s, P>> {
        if !self.checkout_dir_exists() {
            let working_dir_path: &Path = self.working_dir_path.as_ref();
            bail!(
                "missing the git working directory at {:?}; \
                 please run: `cd {:?}; git clone {}; cd -`",
                working_dir_path,
                working_dir_path
                    .parent()
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| PathBuf::from("???")),
                self.supposed_upstream_git_url
            )
        }
        Ok(CheckedCheckoutContext1 {
            parent: self,
            branch_is_checked: AtomicBool::new(false).into(),
        })
    }

    /// See docs on `CheckedCheckoutContext1::check2`; this just calls
    /// `self.check1()?.check2()`.
    pub fn check2(self) -> Result<CheckedCheckoutContext2<'s, P>> {
        self.check1()?.check2()
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
        let current_branch = git_branch_show_current(self.working_dir_path())?;
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

    fn git_remote_get_default(&self) -> Result<String> {
        git_remote_get_default_for_branch(self.working_dir_path(), self.branch_name)?.ok_or_else(
            || {
                anyhow!(
                    "branch {:?} in {:?} does not have a default remote set, \
                     you can't push because of that",
                    self.branch_name,
                    self.working_dir_path()
                )
            },
        )
    }
}

#[derive(Debug, Clone)]
pub struct CheckedCheckoutContext2<'s, P: AsRef<Path>> {
    parent: CheckedCheckoutContext1<'s, P>,
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
