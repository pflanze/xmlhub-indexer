use std::{
    borrow::Cow,
    ffi::{OsStr, OsString},
    fmt::{Debug, Display},
    io::{BufRead, BufReader, Read},
    os::unix::prelude::OsStrExt,
    path::{Path, PathBuf},
    process::{Child, ChildStdout, ExitStatus},
    sync::Arc,
};

use anyhow::{anyhow, bail, Context, Result};

pub use crate::base_and_rel_path::BaseAndRelPath;
use crate::{
    command::{run, run_outputs, run_stdout, spawn, Capturing},
    flattened::Flattened,
    path_util::AppendToPath,
    util::contains_bytes,
};

#[derive(Debug, Clone)]
pub struct GitWorkingDir {
    pub working_dir_path: Arc<PathBuf>,
}

impl From<PathBuf> for GitWorkingDir {
    fn from(value: PathBuf) -> Self {
        GitWorkingDir {
            working_dir_path: Arc::new(value),
        }
    }
}

/// Execute the external "git" command with `base_path` as its current
/// directory and with the given arguments. Returns true when git
/// exited with code 0, false if 1; returns an error for other exit
/// codes or errors.
pub fn _git<S: AsRef<OsStr> + Debug>(
    working_dir: &Path,
    arguments: &[S],
    quiet: bool,
) -> Result<bool> {
    run(
        working_dir,
        "git",
        arguments,
        &[("PAGER", "")],
        &[0, 1],
        if quiet {
            Capturing::stdout()
        } else {
            Capturing::none()
        },
    )
}

pub fn git_clone<'s, P: AsRef<Path>, U: AsRef<OsStr>, SF: AsRef<OsStr>>(
    parent_dir: P,
    clone_opts: impl IntoIterator<Item = &'s str>,
    url: U,
    subdir_filename: SF,
    quiet: bool,
) -> Result<GitWorkingDir> {
    let parent_dir = parent_dir.as_ref().to_owned();
    let clone = OsString::from("clone");
    let mut arguments = vec![&*clone];
    for arg in clone_opts {
        arguments.push(arg.as_ref());
    }
    arguments.push(url.as_ref());
    arguments.push(subdir_filename.as_ref());
    let done = _git(&parent_dir, &arguments, quiet)?;
    if done {
        Ok(GitWorkingDir {
            working_dir_path: Arc::new(parent_dir.append(subdir_filename.as_ref())),
        })
    } else {
        bail!("git clone failed, exited with code 1")
    }
}

impl GitWorkingDir {
    pub fn working_dir_path_ref(&self) -> &Path {
        &self.working_dir_path
    }

    pub fn working_dir_path_arc(&self) -> Arc<PathBuf> {
        self.working_dir_path.clone()
    }

    /// Execute the external "git" command with `base_path` as its current
    /// directory and with the given arguments. Returns true when git
    /// exited with code 0, false if 1; returns an error for other exit
    /// codes or errors.
    pub fn git<S: AsRef<OsStr> + Debug>(&self, arguments: &[S], quiet: bool) -> Result<bool> {
        _git(self.working_dir_path_ref(), arguments, quiet)
    }

    /// Only succeeds if Git exited with code 0.
    pub fn git_stdout<S: AsRef<OsStr> + Debug>(&self, arguments: &[S]) -> Result<Vec<u8>> {
        run_stdout(
            self.working_dir_path_ref(),
            "git",
            arguments,
            &[("PAGER", "")],
            &[0],
        )
        .map(|o| o.output.stdout)
    }

    /// Only succeeds if Git exited with one of the given exit codes,
    /// returning truthy, too.
    pub fn git_stdout_accepting<S: AsRef<OsStr> + Debug>(
        &self,
        arguments: &[S],
        acceptable_status_codes: &[i32],
    ) -> Result<(bool, Vec<u8>)> {
        let o = run_stdout(
            self.working_dir_path_ref(),
            "git",
            arguments,
            &[("PAGER", "")],
            acceptable_status_codes,
        )?;
        Ok((o.truthy, o.output.stdout))
    }

    /// Retrieve the output from a Git command as utf-8 decoded string,
    /// with leading and trailing whitespace removed.
    pub fn git_stdout_string_trimmed_accepting<S: AsRef<OsStr> + Debug>(
        &self,
        arguments: &[S],
        acceptable_status_codes: &[i32],
    ) -> Result<(bool, String)> {
        let (truthy, bytes) = self.git_stdout_accepting(arguments, acceptable_status_codes)?;
        let x = String::from_utf8(bytes)?;
        Ok((truthy, x.trim().into()))
    }

    /// Retrieve the output from a Git command as utf-8 decoded string,
    /// with leading and trailing whitespace removed.
    pub fn git_stdout_string_trimmed<S: AsRef<OsStr> + Debug>(
        &self,
        arguments: &[S],
    ) -> Result<String> {
        let bytes: Vec<u8> = self.git_stdout(arguments)?;
        let x = String::from_utf8(bytes)?;
        Ok(x.trim().into())
    }

    /// Retrieve the output from a Git command as utf-8 decoded string,
    /// with leading and trailing whitespace removed; return the empty
    /// string as None.
    pub fn git_stdout_optional_string_trimmed<S: AsRef<OsStr> + Debug>(
        &self,
        arguments: &[S],
    ) -> Result<Option<String>> {
        let x = self.git_stdout_string_trimmed(arguments)?;
        Ok(if x.is_empty() { None } else { Some(x) })
    }

    /// Get the name of the checked-out branch, if any.
    pub fn git_branch_show_current(&self) -> Result<Option<String>> {
        self.git_stdout_optional_string_trimmed(&["branch", "--show-current"])
    }

    /// Get the name of the checked-out branch, if any.
    pub fn git_describe<S: AsRef<OsStr> + Debug>(&self, arguments: &[S]) -> Result<String> {
        let arguments: Vec<OsString> = arguments.iter().map(|v| v.as_ref().to_owned()).collect();
        let all_args = [vec![OsString::from("describe")], arguments].flattened();
        self.git_stdout_string_trimmed(&all_args)
    }

    pub fn get_head_commit_id(&self) -> Result<String> {
        self.git_stdout_string_trimmed(&["rev-parse", "HEAD"])
    }

    pub fn git_ls_files(&self) -> Result<Vec<BaseAndRelPath>> {
        let stdout = self.git_stdout(&["ls-files", "-z"])?;
        let base_path = self.working_dir_path_arc();
        stdout
            .split(|b| *b == b'\0')
            .map(|bytes| -> Result<_> {
                let rel_path = std::str::from_utf8(bytes)
                    .with_context(|| {
                        anyhow!(
                            "decoding git ls-files output as unicode from directory {:?}: {:?}",
                            base_path.to_string_lossy(),
                            String::from_utf8_lossy(bytes)
                        )
                    })?
                    .into();
                Ok(BaseAndRelPath {
                    base_path: Some(base_path.clone()),
                    rel_path,
                })
            })
            .collect::<Result<Vec<_>>>()
    }
}

#[derive(Debug)]
pub struct GitStatusItem {
    pub x: char,
    pub y: char,
    /// Could include "->" for symlinks
    pub path: String,
    pub target_path: Option<String>,
}

impl GitStatusItem {
    /// If `paranoid` is true, only returns true if both x and y are
    /// '?', otherwise if either is.
    pub fn is_untracked(&self, paranoid: bool) -> bool {
        // According to documentation and observation both are '?' at
        // the same time, and never only one of them '?'. Which way to
        // check?
        if paranoid {
            self.x == '?' && self.y == '?'
        } else {
            self.x == '?' || self.y == '?'
        }
    }
}

impl Display for GitStatusItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{}{}  {}", self.x, self.y, self.path))?;
        if let Some(target_path) = &self.target_path {
            f.write_fmt(format_args!("-> {}", target_path))?;
        }
        Ok(())
    }
}

fn parse_git_status_record(line: &str) -> Result<GitStatusItem> {
    let mut cs = line.chars();
    let x = cs.next().ok_or_else(|| anyhow!("can't parse c0"))?;
    let y = cs.next().ok_or_else(|| anyhow!("can't parse c1"))?;
    let c2 = cs.next().ok_or_else(|| anyhow!("can't parse c2"))?;
    if c2 != ' ' {
        bail!("c2 {c2:?} is not space, x={x:?}, y={y:?}")
    }
    let path: String = cs.collect();
    Ok(GitStatusItem {
        x,
        y,
        path,
        target_path: None,
    })
}

impl GitWorkingDir {
    pub fn git_status(&self) -> Result<Vec<GitStatusItem>> {
        let decode_line = |line_bytes| {
            std::str::from_utf8(line_bytes).with_context(|| {
                anyhow!(
                    "decoding git status output as unicode from directory {:?}: {:?}",
                    self.working_dir_path,
                    String::from_utf8_lossy(line_bytes)
                )
            })
        };
        let stdout = self.git_stdout(&["status", "-z"])?;
        let mut output = Vec::new();
        let mut lines = stdout.split(|b| *b == b'\0');
        while let Some(line_bytes) = lines.next() {
            if line_bytes.is_empty() {
                // Happens if stdout is empty!
                continue;
            }
            let line = decode_line(line_bytes)?;
            let record = parse_git_status_record(line).with_context(|| {
                anyhow!(
                    "decoding git status output from directory {:?}: {:?}",
                    self.working_dir_path,
                    String::from_utf8_lossy(line_bytes)
                )
            })?;
            if record.x == 'R' {
                let line_bytes = lines.next().ok_or_else(|| {
                    anyhow!(
                        "missing git status target path entry after 'R' \
                     for record {record:?}, \
                     from directory {:?}: {:?}",
                        self.working_dir_path,
                        String::from_utf8_lossy(line_bytes)
                    )
                })?;
                let line2 = decode_line(line_bytes)?;
                output.push(GitStatusItem {
                    x: record.x,
                    y: record.y,
                    path: record.path,
                    target_path: Some(line2.into()),
                });
            } else {
                output.push(record);
            }
        }
        Ok(output)
    }
}

/// A single entry returned by the `GitLogIterator` as returned from
/// `git_log`.
#[derive(Debug)]
#[non_exhaustive]
pub struct GitLogEntry {
    pub commit: String, // [u8; 20] ?,
    pub merge: Option<String>,
    pub author: String,
    pub date: String,
    pub message: String,
    // files? Ignore for now
}

pub trait ChildWaiter {
    fn child_wait(&mut self) -> anyhow::Result<ExitStatus>;
}

impl ChildWaiter for Child {
    fn child_wait(&mut self) -> anyhow::Result<ExitStatus> {
        Ok(self.wait()?)
    }
}

pub struct GitLogIterator<R: Read, C: ChildWaiter> {
    child: C,
    stdout: BufReader<R>,
    // The "commit " line if it was read in the previous iteration
    left_over: Option<String>,
}

impl<R: Read, C: ChildWaiter> Iterator for GitLogIterator<R, C> {
    type Item = Result<GitLogEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut line = String::new();
        const NUM_PARTS: usize = 5; // one for each field
        let mut parts: [Option<String>; NUM_PARTS] = Default::default();
        let parts_to_entry = |mut parts: [Option<String>; NUM_PARTS]| {
            Some(Ok(GitLogEntry {
                commit: parts[0].take().unwrap(),
                merge: parts[1].take(),
                author: parts[2].take().unwrap(),
                date: parts[3].take().unwrap(),
                message: parts[4].take().unwrap(),
            }))
        };
        // Index for the part == field.
        let mut parts_i = 0;
        let try_finish = |parts_i: usize, parts: [Option<String>; NUM_PARTS]| {
            if parts_i == NUM_PARTS || parts_i == NUM_PARTS - 1 {
                parts_to_entry(parts)
            } else if parts_i == 0 {
                None
            } else {
                Some(Err(anyhow!(
                    "unfinished entry reading from git log, parts_i = {parts_i}"
                )))
            }
        };
        loop {
            let is_eof = if let Some(left_over) = self.left_over.take() {
                line = left_over;
                false
            } else {
                match self.stdout.read_line(&mut line) {
                    Ok(num_bytes) => num_bytes == 0,
                    Err(e) => return Some(Err(e).with_context(|| anyhow!("reading from git log"))),
                }
            };
            if is_eof {
                match (|| {
                    let status = self.child.child_wait()?;
                    if !status.success() {
                        bail!("exited with non-success status {status:?}")
                    }
                    Ok(())
                })() {
                    Ok(()) => return try_finish(parts_i, parts),
                    Err(e) => return Some(Err(e).with_context(|| anyhow!("finishing git log"))),
                }
            }
            if line.starts_with("commit ") {
                if parts_i == 0 {
                    parts[parts_i] = Some(line["commit ".as_bytes().len()..].trim().to_string());
                } else {
                    self.left_over = Some(line);
                    return try_finish(parts_i, parts);
                }
                parts_i += 1;
            } else {
                match parts_i {
                    0 => unreachable!(),
                    1 | 2 | 3 => {
                        // Merge, Author and Date
                        if parts_i == 1 && !line.starts_with("Merge") {
                            // There is no Merge, just Author and Date
                            parts_i += 1;
                        }
                        if let Some((key, val)) = line.split_once(':') {
                            let (expected_key, valref) = match parts_i {
                                1 => ("Merge", &mut parts[parts_i]),
                                2 => ("Author", &mut parts[parts_i]),
                                3 => ("Date", &mut parts[parts_i]),
                                _ => unreachable!(),
                            };
                            if key != expected_key {
                                return Some(Err(anyhow!(
                                    "expected key {expected_key:?}, but got \
                                     {key:?} from git log on line {parts_i}: {line:?}"
                                )));
                            }
                            *valref = Some(val.trim().into());
                        } else {
                            return Some(Err(anyhow!(
                                "expected `Key: val` on line {parts_i} \
                                 of git log entry, got: {line:?}"
                            )));
                        }
                        parts_i += 1;
                    }
                    4 => {
                        // Commit message
                        if line == "\n" {
                            // ignore; sigh
                        } else if let Some(rest) = line.strip_prefix("    ") {
                            if parts[parts_i].is_none() {
                                parts[parts_i] = Some(String::new());
                            }
                            let message = parts[parts_i].as_mut().unwrap();
                            message.push_str(rest);
                        } else if line.starts_with(':') {
                            // ignore for now; but switch forward
                            parts_i += 1;
                        } else {
                            return Some(Err(anyhow!(
                                "expected commit message or `:...` on line {parts_i} \
                                 of git log entry, got: {line:?}"
                            )));
                        }
                    }
                    5 => {
                        // in ":" file part, ignore
                    }
                    _ => unreachable!(),
                }
            }
            line.clear();
        }
    }
}

impl GitWorkingDir {
    /// Git log will already receive appropriate formatting options
    /// (`--raw` or `--format..`), don't give any!
    pub fn git_log<S: AsRef<OsStr> + Debug>(
        &self,
        arguments: &[S],
    ) -> Result<GitLogIterator<ChildStdout, Child>> {
        let mut all_arguments: Vec<&OsStr> = vec![
            OsStr::from_bytes("log".as_bytes()),
            OsStr::from_bytes("--raw".as_bytes()),
        ];
        for arg in arguments {
            all_arguments.push(arg.as_ref());
        }
        let mut child = spawn(
            self.working_dir_path_ref(),
            "git",
            &all_arguments,
            &[("PAGER", "")],
            Capturing::stdout(),
        )?;
        let stdout = BufReader::new(child.stdout.take().expect("specified"));
        Ok(GitLogIterator {
            child,
            stdout,
            left_over: None,
        })
    }

    /// Resolve the given reference. If `to_commit` is true, resolves to a
    /// commit id. Returns None if the reference doesn't exist / can't be
    /// resolved (details?).
    pub fn git_rev_parse(&self, name: &str, to_commit: bool) -> Result<Option<String>> {
        let full_name: Cow<str> = if to_commit {
            format!("{name}^{{commit}}").into()
        } else {
            name.into()
        };
        let outputs = run_outputs(
            self.working_dir_path_ref(),
            "git",
            &["rev-parse", &full_name],
            &[("PAGER", "")],
            &[0, 128],
        )?;
        if outputs.truthy {
            let stdout = std::str::from_utf8(&outputs.stdout)?;
            let commit = stdout.trim();
            if commit.is_empty() {
                bail!("`git rev-parse {full_name:?}` returned the empty string")
            }
            Ok(Some(commit.into()))
        } else if contains_bytes(&outputs.stderr, b": unknown revision") {
            Ok(None)
        } else {
            bail!("`git rev-parse {full_name:?}`: {outputs}")
        }
    }

    /// Create an annotated or signed Git tag. Returns whether the tag has
    /// been created, `false` means the tag already exists on the same
    /// commit (an error is returned if it exists on another commit). Does
    /// not check whether the tag message is the same, though!
    pub fn git_tag(
        &self,
        tag_name: &str,
        revision: Option<&str>,
        message: &str,
        sign: bool,
        local_user: Option<&str>,
    ) -> Result<bool> {
        let clean_local_user: String;
        let mut args = vec![
            "tag",
            if sign { "-s" } else { "-a" },
            tag_name,
            "-m",
            message,
        ];
        if let Some(local_user) = local_user {
            // Meh, "GPG Keychain" (gpgtools.org) on macOS copies the
            // fingerprint with non-breaking spaces to the clipboard.
            clean_local_user = local_user
                .chars()
                .map(|c| if c == '\u{a0}' { ' ' } else { c })
                .collect();
            args.push("--local-user");
            args.push(&clean_local_user);
        }
        if let Some(revision) = revision {
            args.push(revision);
        }

        let explain = |e| {
            let hint = if local_user.is_none() {
                "-- NOTE: if you get 'gpg failed to sign the data', try giving the \
             local-user argument"
            } else {
                ""
            };
            let base_path = self.working_dir_path_ref();
            Err(e).with_context(|| anyhow!("running git {args:?} in {base_path:?}{hint}"))
        };
        match run_outputs(
            self.working_dir_path_ref(),
            "git",
            &args,
            &[("PAGER", "")],
            &[0, 128],
        ) {
            Err(e) => explain(e),
            Ok(outputs) => {
                if outputs.truthy {
                    Ok(true)
                } else {
                    if contains_bytes(&outputs.stderr, b"already exists") {
                        let want_revision = revision.unwrap_or("HEAD");
                        let want_commitid =
                            self.git_rev_parse(want_revision, true)?.ok_or_else(|| {
                                anyhow!("given revision {want_revision:?} does not resolve")
                            })?;
                        let existing_commitid =
                            self.git_rev_parse(tag_name, true)?.ok_or_else(|| {
                                anyhow!(
                                    "`git tag ..` said tag {tag_name:?} already exists, \
                                 but that name does not resolve"
                                )
                            })?;
                        if want_commitid == existing_commitid {
                            Ok(false)
                        } else {
                            bail!(
                                "asked to create tag {tag_name:?} to commit {want_commitid:?}, \
                             but that tag name already exists for commit {existing_commitid:?}"
                            )
                        }
                    } else {
                        // (How to make a proper error? Ideally `Outputs`
                        // would hold it, created by the function that
                        // returns it?) Hack:
                        explain(anyhow!("{outputs}"))
                    }
                }
            }
        }
    }

    /// Get the name of the remote for the given branch
    pub fn git_remote_get_default_for_branch(&self, branch_name: &str) -> Result<Option<String>> {
        let config_key = format!("branch.{branch_name}.remote");
        let (truthy, string) =
            self.git_stdout_string_trimmed_accepting(&["config", "--get", &config_key], &[0, 1])?;
        if truthy {
            if string.is_empty() {
                let base_path = self.working_dir_path_ref();
                bail!(
                    "the string returned by `git config --get {config_key:?}` \
                 in {base_path:?} is empty"
                )
            } else {
                Ok(Some(string))
            }
        } else {
            Ok(None)
        }
    }

    /// Push the given refspecs (like branch or tag names) to the given
    /// repository (like remote name).
    pub fn git_push<S: AsRef<OsStr> + Debug>(
        &self,
        repository: &str,
        refspecs: &[S],
        quiet: bool,
    ) -> Result<()> {
        let mut args: Vec<&OsStr> = vec!["push".as_ref(), repository.as_ref()];
        for v in refspecs {
            args.push(v.as_ref());
        }
        if !self.git(&args, quiet)? {
            let base_path = self.working_dir_path_ref();
            bail!("git {args:?} in {base_path:?} failed")
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GitResetMode {
    Soft,
    Mixed,
    Hard,
    Merge,
    Keep,
}

impl GitResetMode {
    pub fn to_str(self) -> &'static str {
        match self {
            GitResetMode::Soft => "--soft",
            GitResetMode::Mixed => "--mixed",
            GitResetMode::Hard => "--hard",
            GitResetMode::Merge => "--merge",
            GitResetMode::Keep => "--keep",
        }
    }
}

impl Default for GitResetMode {
    fn default() -> Self {
        Self::Mixed
    }
}

impl GitWorkingDir {
    pub fn git_reset<S: AsRef<OsStr> + Debug>(
        &self,
        mode: GitResetMode,
        options: &[S],
        refspec: &str,
        quiet: bool,
    ) -> Result<()> {
        let mut args: Vec<&OsStr> = vec!["reset".as_ref(), mode.to_str().as_ref()];
        for opt in options {
            args.push(opt.as_ref())
        }
        // Add *no* "--" before the refspec or it would mean a path!
        args.push(refspec.as_ref());
        self.git(&args, quiet)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GitObjectType {
    Blob,
    Tree,
    Commit,
    /// Annotated tag object (includes metadata and can point to any other object).
    Tag,
}
impl GitObjectType {
    pub fn to_str(self) -> &'static str {
        match self {
            GitObjectType::Blob => "blob",
            GitObjectType::Tree => "tree",
            GitObjectType::Commit => "commit",
            GitObjectType::Tag => "tag",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GitCatFileMode {
    ShowType,
    ShowSize,
    ShowExists,
    ShowPretty,
    Type(GitObjectType),
}

impl GitCatFileMode {
    pub fn to_str(self) -> &'static str {
        match self {
            GitCatFileMode::ShowType => "-t",
            GitCatFileMode::ShowSize => "-s",
            GitCatFileMode::ShowExists => "-e",
            GitCatFileMode::ShowPretty => "-p",
            GitCatFileMode::Type(t) => t.to_str(),
        }
    }
}

impl GitWorkingDir {
    pub fn git_cat_file(&self, mode: GitCatFileMode, object: &str) -> Result<bool> {
        let args = &["cat-file", mode.to_str(), object];
        self.git(args, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NopWaiter;
    impl ChildWaiter for NopWaiter {
        fn child_wait(&mut self) -> anyhow::Result<ExitStatus> {
            bail!("no wait")
        }
    }

    fn gitlog_iterator_from_str<'s>(s: &'s str) -> impl Iterator<Item = Result<GitLogEntry>> + 's {
        GitLogIterator {
            child: NopWaiter,
            stdout: BufReader::new(s.as_bytes()),
            left_over: None,
        }
    }

    fn t_gitlog_iterator(s: &str) -> Result<()> {
        let mut it = gitlog_iterator_from_str(s);
        let _r = it.next().unwrap()?;
        Ok(())
    }

    #[test]
    fn t0() -> Result<()> {
        t_gitlog_iterator(
            "commit afb6184585974a96688ec42c7f024118fcbc8d86
Author: Christian Jaeger (Mac) <ch@christianjaeger.ch>
Date:   Sun Apr 13 21:26:17 2025 +0200

    regenerate index files via xmlhub
    
    version: 8.1

commit 49a0c5ceed749fc4ec7a7798af56f19447977c56
Author: Marcus Overwater <moverwater@ethz.ch>
Date:   Thu Apr 3 14:59:34 2025 +0200

    Added ReMASTER simulation xml

commit 7ed02856897a8d2a8e9c8887ebf99e6e3c0c1cf7
Author: Louis <louis.duplessis@bsse.ethz.ch>
Date:   Thu Jun 6 11:41:55 2024 +0200

    Initial commit
",
        )
    }

    #[test]
    fn t1() -> Result<()> {
        t_gitlog_iterator(
            "commit 2a1e2fd51372cb1dba8d0b9ed076afa10ea53183
Merge: 49a0c5c b995c14
Author: Marcus Overwater <moverwater@ethz.ch>
Date:   Thu Apr 17 11:25:21 2025 +0200

    Merge branch 'master' of /Users/moverwater/xmlhub

commit afb6184585974a96688ec42c7f024118fcbc8d86
Author: Christian Jaeger (Mac) <ch@christianjaeger.ch>
Date:   Sun Apr 13 21:26:17 2025 +0200

    regenerate index files via xmlhub
    
    version: 8.1

commit 49a0c5ceed749fc4ec7a7798af56f19447977c56
Author: Marcus Overwater <moverwater@ethz.ch>
Date:   Thu Apr 3 14:59:34 2025 +0200

    Added ReMASTER simulation xml

commit 7ed02856897a8d2a8e9c8887ebf99e6e3c0c1cf7
Author: Louis <louis.duplessis@bsse.ethz.ch>
Date:   Thu Jun 6 11:41:55 2024 +0200

    Initial commit
",
        )
    }
}
