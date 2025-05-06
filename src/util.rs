use anyhow::{anyhow, bail, Context, Result};
use itertools::Itertools;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Display,
    fs::{create_dir, File, OpenOptions},
    io::{BufRead, Write},
    io::{BufReader, BufWriter},
    path::Path,
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
            if !last_was_whitespace {
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
    run_stdout_string::<_, &str, &str>(".", "hostname", &[], &[], &[0], true)
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
/// path, if you have a file path, be careful to take the `parent()`
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

/// Create a file and write to it via the given function, with
/// buffering. Flush the file once finished successfully.
pub fn with_output_to_file(
    output_path: &Path,
    writer: impl FnOnce(&mut dyn Write) -> Result<()>,
) -> Result<()> {
    (|| -> Result<()> {
        let mut output = BufWriter::new(File::create(&output_path)?);
        writer(&mut output)?;
        output.flush()?;
        Ok(())
    })()
    .with_context(|| anyhow!("writing to file {output_path:?}"))
}

/// Does the same for bytes that `haystack.contains(needle)` does for
/// strings. (This will be in std in the future:
/// <https://github.com/rust-lang/rust/issues/134149>)
pub fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// Format a sequence of items that can be represented as &str to a
/// string for human consumption; e.g. `[String::from("Hi"),
/// String::from("there")]` => `"\"Hi\", \"there\""`.
pub fn format_string_list<S, L>(sequence: L) -> String
where
    S: AsRef<str>,
    L: IntoIterator<Item = S>,
{
    let iter = sequence.into_iter();
    let items: Vec<String> = iter.map(|v| format!("{:?}", v.as_ref())).collect();
    items.join(", ")
}

#[test]
fn t_format_string_list() {
    assert_eq!(
        format_string_list([String::from("Hi"), String::from("there")]),
        "\"Hi\", \"there\""
    );
}

const MAX_ANCHOR_NAME_LEN: usize = 60;

/// Format a string so that it can be safely used as an anchor name:
/// only alphanumeric characters are preserved, anything else is
/// replaced with underscore. Also, limits the length to
/// MAX_ANCHOR_NAME_LEN (simply cuts off the remainder!). Note that
/// this function does not guarantee an 1:1 mapping even if `s` is
/// shorter.
pub fn format_anchor_name(s: &str) -> String {
    let s = &s[..MAX_ANCHOR_NAME_LEN.min(s.len())];
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

#[test]
fn t_format_anchor_name() {
    let t = format_anchor_name;
    assert_eq!(t("Hi there!"), "Hi_there_");
    assert_eq!(t(""), "");
    assert_eq!(
        t("Format a string so that it can be safely used as an anchor name"),
        "Format_a_string_so_that_it_can_be_safely_used_as_an_anchor_n"
    );
}

// unused?
pub fn bool_to_yes_no(val: bool) -> &'static str {
    if val {
        "yes"
    } else {
        "no"
    }
}

pub fn prefix_lines(lines: &str, prefix: &str) -> String {
    let (lines_no_ending_newline, suffix) = if let Some(s) = lines.strip_suffix("\n") {
        (s, "\n")
    } else {
        (lines, "")
    };
    let mut new = lines_no_ending_newline
        .split("\n")
        .map(|line| format!("{prefix}{line}"))
        .join("\n");
    new.push_str(suffix);
    new
}

#[test]
fn t_prefix_lines() {
    let t = prefix_lines;
    assert_eq!(t("hi", "  "), "  hi");
    assert_eq!(t("hi\n", "  "), "  hi\n");
    assert_eq!(t("hi\nthere", "  "), "  hi\n  there");
    assert_eq!(t("hi\nthere\n", "  "), "  hi\n  there\n");
    assert_eq!(t("\n\n", "  "), "  \n  \n");
}

pub fn strip_prefixes<'s>(s: &'s str, prefixes: &[&str]) -> &'s str {
    let mut s = s;
    for prefix in prefixes {
        s = s.strip_prefix(prefix).unwrap_or(s);
    }
    s
}
