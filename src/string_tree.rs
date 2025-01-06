//! # A string represented by a tree of substrings.

//! This allows to build up a large document recursively without
//! having to copy its parts for each recursion level (and each resize
//! of a string to collect parts to).

use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};

#[derive(Debug)]
pub enum StringTree {
    Leaf(String),
    StaticLeaf(&'static str),
    Branching(Vec<StringTree>),
}

impl StringTree {
    pub fn print_to_string(&self, out: &mut String) {
        match self {
            StringTree::Leaf(s) => out.push_str(s),
            StringTree::StaticLeaf(s) => out.push_str(s),
            StringTree::Branching(vec) => {
                for v in vec {
                    v.print_to_string(out);
                }
            }
        }
    }

    /// Total len in bytes
    pub fn len(&self) -> usize {
        match self {
            StringTree::Leaf(s) => s.len(),
            StringTree::StaticLeaf(s) => s.len(),
            StringTree::Branching(v) => v.iter().map(|s| s.len()).sum(),
        }
    }

    pub fn write_all(&self, out: &mut impl Write) -> Result<(), std::io::Error> {
        // out.write_all_vectored() is unstable, thus make individual
        // write calls instead
        match self {
            StringTree::Leaf(s) => out.write_all(s.as_bytes()),
            StringTree::StaticLeaf(s) => out.write_all(s.as_bytes()),
            StringTree::Branching(vec) => {
                for v in vec {
                    v.write_all(out)?;
                }
                Ok(())
            }
        }
    }

    pub fn write_to_file(&self, path: &Path) -> Result<(), std::io::Error> {
        // Both of these need to copy the strings once; the first
        // knows the size of the buffer to allocate, but needs more
        // temporary space.
        if false {
            let mut out = File::create(&path)?;
            out.write_all(self.to_string().as_bytes())?;
            out.flush()
        } else {
            let mut out = BufWriter::new(File::create(&path)?);
            self.write_all(&mut out)?;
            out.flush()
        }
    }
}

impl ToString for StringTree {
    fn to_string(&self) -> String {
        let mut out = String::with_capacity(self.len());
        self.print_to_string(&mut out);
        out
    }
}

impl From<String> for StringTree {
    fn from(value: String) -> Self {
        StringTree::Leaf(value)
    }
}

impl From<&'static str> for StringTree {
    fn from(value: &'static str) -> Self {
        StringTree::StaticLeaf(value)
    }
}
