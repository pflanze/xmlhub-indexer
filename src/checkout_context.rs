use std::path::Path;

use anyhow::{anyhow, bail, Result};

use crate::git::{git_branch_show_current, git_remote_get_default_for_branch, git_status};

#[derive(Clone)]
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
    pub fn replace_working_dir_path<'p, P2: AsRef<Path>>(
        &'s self,
        path: P2,
    ) -> CheckoutContext<'p, P2>
    where
        's: 'p,
        P: Clone,
        P2: 'p, // ?
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

    pub fn checkout_dir_exists(&self) -> bool {
        let path: &Path = self.working_dir_path.as_ref();
        path.exists()
    }

    pub fn working_dir_path(&self) -> &Path {
        self.working_dir_path.as_ref()
    }

    pub fn check_current_branch(&self) -> Result<()> {
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
        Ok(())
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

    pub fn git_remote_get_default(&self) -> Result<String> {
        git_remote_get_default_for_branch(self.working_dir_path(), self.branch_name)?.ok_or_else(
            || {
                anyhow!(
                    "branch {:?} in {:?} does not have a default remote set, \
                     you can't use the `--push` option because of that",
                    self.branch_name,
                    self.working_dir_path()
                )
            },
        )
    }
}
