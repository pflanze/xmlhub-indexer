use anyhow::{anyhow, bail, Context, Result};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    fmt::Display,
    fs::{create_dir, OpenOptions},
    io::BufReader,
    io::{BufRead, Write},
    path::{Path, PathBuf},
};

use crate::command::run_stdout_string;

pub trait InsertValue<K, V> {
    /// Insert a value into a collection of value that `key` maps to,
    /// creating the collection and the mapping from key if it doesn't
    /// exist yet. Returns whether the value was newly added (false
    /// means `val` was already in there).
    fn insert_value(&mut self, key: K, val: V) -> bool;
}

impl<K: Ord + Clone, V: Ord> InsertValue<K, V> for BTreeMap<K, BTreeSet<V>> {
    fn insert_value(&mut self, key: K, val: V) -> bool {
        if let Some(vals) = self.get_mut(&key) {
            vals.insert(val)
        } else {
            let mut vals = BTreeSet::new();
            vals.insert(val);
            self.insert(key.clone(), vals);
            true
        }
    }
}

/// From a list of values, try to get the one for which an extracted
/// value matches `key`.
pub fn list_get_by_key<'t, K: Eq, T>(
    vals: &'t [T],
    get_key: impl Fn(&T) -> &K,
    key: &K,
) -> Option<&'t T> {
    vals.iter().find(|item| get_key(item) == key)
}

/// Create a new vector that contains copies of the elements of both
/// argument vectors or slices.
pub fn append<T, V1, V2>(a: V1, b: V2) -> Vec<T>
where
    V1: IntoIterator<Item = T>,
    V2: IntoIterator<Item = T>,
{
    let mut vec = Vec::new();
    for v in a {
        vec.push(v);
    }
    for v in b {
        vec.push(v);
    }
    vec
}

/// Replace groups of whitespace characters with a single space each.
pub fn normalize_whitespace(s: &str) -> String {
    let mut result = String::new();
    let mut last_was_whitespace = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if last_was_whitespace {
                ()
            } else {
                result.push(' ');
                last_was_whitespace = true;
            }
        } else {
            result.push(c);
            last_was_whitespace = false;
        }
    }
    result
}

#[cfg(test)]
#[test]
fn t_normalize_whitespace() {
    let t = normalize_whitespace;
    assert_eq!(t("Hi !"), "Hi !");
    assert_eq!(t(""), "");
    assert_eq!(t("Hi  !"), "Hi !");
    assert_eq!(t("  Hi  !\n\n\n"), " Hi ! ");
}

/// Convert a slice of references to a vector that owns the owned
/// versions of the items.
pub fn to_owned_items<O, T: ToOwned<Owned = O> + ?Sized>(vals: &[&T]) -> Vec<O> {
    vals.iter().map(|s| (*s).to_owned()).collect::<Vec<O>>()
}

pub fn remove_file_if_present(path: &str) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(io_err) => match io_err.kind() {
            std::io::ErrorKind::NotFound => Ok(()),
            _ => Err(io_err).with_context(|| "trying to remove file {path:?}"),
        },
    }
}

pub fn prog_version(in_dir: &Path, prog_name: &str) -> Result<String> {
    run_stdout_string(in_dir, prog_name, &["--version"], &[], &[0], true)
}

pub fn hostname() -> Result<String> {
    run_stdout_string::<&str, &str>(&PathBuf::from("."), "hostname", &[], &[], &[0], true)
}

/// Calculate SHA-256 hash sum for the given path (currently by
/// calling the external `sha256sum` binary) as hex string.
pub fn sha256sum<P: AsRef<OsStr>>(base: &Path, path: P) -> Result<String> {
    let stdout = run_stdout_string(base, "sha256sum", &[path.as_ref()], &[], &[0], true)?;
    if let Some(pos) = stdout.find(|c: char| c.is_whitespace()) {
        Ok(stdout[0..pos].to_string())
    } else {
        Ok(stdout)
    }
}

pub fn stringify_error<T: Display, E: Display>(res: Result<T, E>) -> String {
    match res {
        Ok(v) => format!(": {v}"),
        Err(e) => format!(" -- {e}"),
    }
}

pub fn ask_yn(question: &str) -> Result<bool> {
    let mut opts = OpenOptions::new();
    opts.read(true).write(true).create(false);
    let opn = || opts.open("/dev/tty");
    let mut inp = BufReader::new(opn()?);
    let mut outp = opn()?;
    for n in (1..5).rev() {
        write!(outp, "{} (y/n) ", question)?;
        let mut ans = String::new();
        inp.read_line(&mut ans)?;
        if ans.len() > 1 && ans.starts_with("y") {
            return Ok(true);
        } else if ans.len() > 1 && ans.starts_with("n") {
            return Ok(false);
        }
        writeln!(outp, "Please answer with y or n, {} tries left", n)?;
    }
    bail!("Could not get an answer to the question {:?}", question)
}

/// Create the given directory if it doesn't exist and `levels` is at
/// least 1, as well as the given number of `levels - 1` above
/// it. (But also see `create_dir_all`.) This expects a directory
/// path, you have a file path, be careful to take the `parent()`
/// first!
pub fn create_dir_levels_if_necessary(dir_path: &Path, levels: u32) -> Result<()> {
    // eprintln!("create_dir_levels_if_necessary({dir_path:?}, {levels})");
    if levels == 0 {
        Ok(())
    } else {
        if let Some(parent) = dir_path.parent() {
            create_dir_levels_if_necessary(parent, levels - 1)?
        }
        // eprintln!("create_dir({dir_path:?})...");
        match create_dir(dir_path) {
            Ok(_) => Ok(()),
            Err(e) => match e.kind() {
                std::io::ErrorKind::AlreadyExists => Ok(()),
                _ => Err(e).with_context(|| anyhow!("creating directory {dir_path:?}")),
            },
        }
    }
}

// Like `haystack.contains(needle)` for strings. (This will be in std
// in the future: https://github.com/rust-lang/rust/issues/134149)
pub fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}
