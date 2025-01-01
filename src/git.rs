use std::{
    borrow::Cow,
    ffi::{OsStr, OsString},
    fmt::Debug,
    io::{BufRead, BufReader},
    os::unix::prelude::OsStrExt,
    path::{Path, PathBuf},
    process::{Child, ChildStdout},
    sync::Arc,
};

use anyhow::{anyhow, bail, Context, Result};

use crate::{
    command::{run, run_outputs, run_stdout, spawn, Capturing},
    flattened::Flattened,
    util::contains_bytes,
};

#[derive(Debug, Clone)]
pub struct RelPathWithBase {
    base_path: Option<Arc<PathBuf>>,
    rel_path: PathBuf,
}

impl RelPathWithBase {
    pub fn new(base_path: Option<Arc<PathBuf>>, rel_path: PathBuf) -> Self {
        Self {
            base_path,
            rel_path,
        }
    }

    pub fn full_path(&self) -> PathBuf {
        if let Some(path) = &self.base_path {
            let mut path = (**path).to_owned();
            path.push(&self.rel_path);
            path
        } else {
            self.rel_path.clone()
        }
    }

    pub fn extension(&self) -> Option<&OsStr> {
        self.rel_path.extension()
    }

    pub fn rel_path(&self) -> &str {
        self.rel_path
            // XXX what happens on Windows? UTF-16 always needs recoding, no?
            .to_str()
            .expect("always works since created from str")
    }
}

/// Returns true when git exited with code 0, false if 1; returns an
/// error for other exit codes or errors.
pub fn git<S: AsRef<OsStr> + Debug>(base_path: &Path, arguments: &[S]) -> Result<bool> {
    run(base_path, "git", arguments, &[("PAGER", "")], &[0, 1])
}

/// Only succeeds if Git exited with code 0.
pub fn git_stdout<S: AsRef<OsStr> + Debug>(base_path: &Path, arguments: &[S]) -> Result<Vec<u8>> {
    run_stdout(base_path, "git", arguments, &[("PAGER", "")], &[0]).map(|o| o.output.stdout)
}

/// Only succeeds if Git exited with one of the given exit codes,
/// returning truthy, too.
pub fn git_stdout_accepting<S: AsRef<OsStr> + Debug>(
    base_path: &Path,
    arguments: &[S],
    acceptable_status_codes: &[i32],
) -> Result<(bool, Vec<u8>)> {
    let o = run_stdout(
        base_path,
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
    base_path: &Path,
    arguments: &[S],
    acceptable_status_codes: &[i32],
) -> Result<(bool, String)> {
    let (truthy, bytes) = git_stdout_accepting(base_path, arguments, acceptable_status_codes)?;
    let x = String::from_utf8(bytes)?;
    Ok((truthy, x.trim().into()))
}

/// Retrieve the output from a Git command as utf-8 decoded string,
/// with leading and trailing whitespace removed.
pub fn git_stdout_string_trimmed<S: AsRef<OsStr> + Debug>(
    base_path: &Path,
    arguments: &[S],
) -> Result<String> {
    let bytes: Vec<u8> = git_stdout(base_path, arguments)?;
    let x = String::from_utf8(bytes)?;
    Ok(x.trim().into())
}

/// Retrieve the output from a Git command as utf-8 decoded string,
/// with leading and trailing whitespace removed; return the empty
/// string as None.
pub fn git_stdout_optional_string_trimmed<S: AsRef<OsStr> + Debug>(
    base_path: &Path,
    arguments: &[S],
) -> Result<Option<String>> {
    let x = git_stdout_string_trimmed(base_path, arguments)?;
    Ok(if x.is_empty() { None } else { Some(x) })
}

/// Get the name of the checked-out branch, if any.
pub fn git_branch_show_current(base_path: &Path) -> Result<Option<String>> {
    git_stdout_optional_string_trimmed(base_path, &["branch", "--show-current"])
}

/// Get the name of the checked-out branch, if any.
pub fn git_describe<S: AsRef<OsStr> + Debug>(base_path: &Path, arguments: &[S]) -> Result<String> {
    let arguments: Vec<OsString> = arguments.iter().map(|v| v.as_ref().to_owned()).collect();
    let all_args = [vec![OsString::from("describe")], arguments].flattened();
    git_stdout_string_trimmed(base_path, &all_args)
}

pub fn git_ls_files(base_path: &Path) -> Result<Vec<RelPathWithBase>> {
    let stdout = git_stdout(base_path, &["ls-files", "-z"])?;
    let base_path = Arc::new(base_path.to_owned());
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
            Ok(RelPathWithBase {
                base_path: Some(base_path.clone()),
                rel_path,
            })
        })
        .collect::<Result<Vec<_>>>()
}

#[derive(Debug)]
pub struct GitStatusItem {
    pub x: char,
    pub y: char,
    /// Could include "->" for symlinks
    pub path: String,
    pub target_path: Option<String>,
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

pub fn git_status(base_path: &Path) -> Result<Vec<GitStatusItem>> {
    let decode_line = |line_bytes| {
        std::str::from_utf8(line_bytes).with_context(|| {
            anyhow!(
                "decoding git status output as unicode from directory {:?}: {:?}",
                base_path.to_string_lossy(),
                String::from_utf8_lossy(line_bytes)
            )
        })
    };
    let stdout = git_stdout(base_path, &["status", "-z"])?;
    let mut output = Vec::new();
    let mut lines = stdout.split(|b| *b == b'\0');
    while let Some(line_bytes) = lines.next() {
        if line_bytes.is_empty() {
            // Happens if stdout is empty!
            continue;
        }
        let line = decode_line(line_bytes)?;
        let record = parse_git_status_record(&line).with_context(|| {
            anyhow!(
                "decoding git status output from directory {:?}: {:?}",
                base_path.to_string_lossy(),
                String::from_utf8_lossy(line_bytes)
            )
        })?;
        if record.x == 'R' {
            let line_bytes = lines.next().ok_or_else(|| {
                anyhow!(
                    "missing git status target path entry after 'R' \
                     for record {record:?}, \
                     from directory {:?}: {:?}",
                    base_path.to_string_lossy(),
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

/// Do not pattern-match fully on this struct, as fields may be added
/// at any time!
#[derive(Debug)]
pub struct GitLogEntry {
    pub commit: String, // [u8; 20] ?,
    pub author: String,
    pub date: String,
    pub message: String,
    // files? Ignore for now
}

pub struct GitLogIterator {
    child: Child,
    stdout: BufReader<ChildStdout>,
    // The "commit " line if it was read in the previous iteration
    left_over: Option<String>,
}

impl Iterator for GitLogIterator {
    type Item = Result<GitLogEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut line = String::new();
        const NUM_PARTS: usize = 4; // one for each field
        let mut parts: [Option<String>; NUM_PARTS] = Default::default();
        let parts_to_entry = |mut parts: [Option<String>; NUM_PARTS]| {
            Some(Ok(GitLogEntry {
                commit: parts[0].take().unwrap(),
                author: parts[1].take().unwrap(),
                date: parts[2].take().unwrap(),
                message: parts[3].take().unwrap(),
            }))
        };
        let mut parts_i = 0;
        let try_finish = |parts_i: usize, parts: [Option<String>; NUM_PARTS]| {
            if parts_i == NUM_PARTS {
                return parts_to_entry(parts);
            } else if parts_i == 0 {
                return None;
            } else {
                return Some(Err(anyhow!("unfinished entry reading from git log")));
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
                    let status = self.child.wait()?;
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
                    1 | 2 => {
                        // Author and Date
                        if let Some((key, val)) = line.split_once(':') {
                            let (expected_key, valref) = match parts_i {
                                1 => ("Author", &mut parts[parts_i]),
                                2 => ("Date", &mut parts[parts_i]),
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
                    3 => {
                        // Commit message
                        if line == "\n" {
                            // ignore; sigh
                        } else if line.starts_with("    ") {
                            if parts[parts_i].is_none() {
                                parts[parts_i] = Some(String::new());
                            }
                            let message = parts[parts_i].as_mut().unwrap();
                            message.push_str(&line[4..]);
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
                    4 => {
                        // in ":" file part, ignore
                    }
                    _ => unreachable!(),
                }
            }
            line.clear();
        }
    }
}

/// Git log will already receive appropriate formatting options
/// (`--raw` or `--format..`), don't give any!
pub fn git_log<S: AsRef<OsStr> + Debug>(
    base_path: &Path,
    arguments: &[S],
) -> Result<GitLogIterator> {
    let mut all_arguments: Vec<&OsStr> = vec![
        OsStr::from_bytes("log".as_bytes()),
        OsStr::from_bytes("--raw".as_bytes()),
    ];
    for arg in arguments {
        all_arguments.push(arg.as_ref());
    }
    let mut child = spawn(
        base_path,
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
pub fn git_rev_parse(base_path: &Path, name: &str, to_commit: bool) -> Result<Option<String>> {
    let full_name: Cow<str> = if to_commit {
        format!("{name}^{{commit}}").into()
    } else {
        name.into()
    };
    let outputs = run_outputs(
        base_path,
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
    base_path: &Path,
    tag_name: &str,
    revision: Option<&str>,
    message: &str,
    sign: bool,
    local_user: Option<&str>,
) -> Result<bool> {
    let mut args = vec![
        "tag",
        if sign { "-s" } else { "-a" },
        tag_name,
        "-m",
        message,
    ];
    if let Some(local_user) = local_user {
        args.push("--local-user");
        args.push(local_user);
    }
    if let Some(revision) = revision {
        args.push(revision);
    }

    let explain = |e| {
        if local_user.is_none() {
            Err(e).with_context(|| {
                anyhow!(
                    "if you get 'gpg failed to sign the data', try \
                         giving the local-user argument"
                )
            })
        } else {
            Err(e)
        }
    };
    match run_outputs(base_path, "git", &args, &[("PAGER", "")], &[0, 128]) {
        Err(e) => explain(e),
        Ok(outputs) => {
            if outputs.truthy {
                Ok(true)
            } else {
                if contains_bytes(&outputs.stderr, b"already exists") {
                    let want_revision = revision.unwrap_or("HEAD");
                    let want_commitid =
                        git_rev_parse(base_path, want_revision, true)?.ok_or_else(|| {
                            anyhow!("given revision {want_revision:?} does not resolve")
                        })?;
                    let existing_commitid =
                        git_rev_parse(base_path, tag_name, true)?.ok_or_else(|| {
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
pub fn git_remote_get_default_for_branch(
    base_path: &Path,
    branch_name: &str,
) -> Result<Option<String>> {
    let config_key = format!("branch.{branch_name}.remote");
    let (truthy, string) =
        git_stdout_string_trimmed_accepting(base_path, &["config", "--get", &config_key], &[0, 1])?;
    if truthy {
        if string.is_empty() {
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
