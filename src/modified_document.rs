//! Modify a string via a set of deletions and inserts. Collects those
//! modifications and only on request carries them all out in one go.

//! Modifications are not layered: it is not possible to insert a
//! string and then afterwards delete part of that inserted string.
//! Modification positions are always referring to the original
//! string.

//! TODO?: allow region overlaps, however?: parts deleted multiple
//! times are just deleted (currently raises an error). Deletions that
//! cross over a position where an insert is done, obviates the insert
//! (regardless of the order of modifications) (currently raises an
//! error).

use std::{io::Write, ops::Range};

use anyhow::{bail, Result};

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Modification {
    /// Delete the given byte range of the original backing string.
    Delete(Range<usize>),
    /// Insert the given string (which must represent a valid XML
    /// fragment) at the byte position in the original backing string.
    Insert(usize, Box<str>),
    // Replace: just use Delete and Insert?
}

impl Modification {
    /// The start position
    pub fn start(&self) -> usize {
        match self {
            Modification::Delete(range) => range.start,
            Modification::Insert(start, _) => *start,
        }
    }

    /// The position in the document after this modification
    pub fn end(&self) -> usize {
        match self {
            Modification::Delete(range) => range.end,
            Modification::Insert(start, _) => *start,
        }
    }

    /// An index for ordering modifications when they have the same
    /// start position. Higher numbers mean sorting in the same
    /// direction as higher start indices.
    pub fn ordering(&self) -> u32 {
        match self {
            Modification::Delete(_) => 1,
            Modification::Insert(_, _) => 0,
        }
    }
}

impl Ord for Modification {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.start()
            .cmp(&other.start())
            .then_with(|| self.ordering().cmp(&other.ordering()))
    }
}

impl PartialOrd for Modification {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

pub struct ModifiedDocument<'d> {
    string: &'d str,
    modifications: Vec<Modification>,
}

impl<'d> ModifiedDocument<'d> {
    pub fn new(document: &'d str) -> Self {
        Self {
            string: document,
            modifications: Vec::new(),
        }
    }

    pub fn original_str(&self) -> &'d str {
        self.string
    }

    /// Whether edits have been carried out; no check is done whether
    /// edits were done that yield the original document. For the
    /// latter, use `to_string_and_modified` instead.
    pub fn has_modifiations(&self) -> bool {
        !self.modifications.is_empty()
    }

    /// Add a modification; each modification's positions are with
    /// respect to the original document, independent of those pushed
    /// before. Overlaps lead to an error when applying the
    /// modifications. Positions must match UTF-8 boundaries,
    /// otherwise a panic will happen in `to_string`, and `write_to`
    /// will output invalid UTF-8!
    pub fn push(&mut self, modification: Modification) {
        self.modifications.push(modification)
    }

    pub fn sort_and_check_modifications(&mut self) -> Result<()> {
        self.modifications.sort();
        // Check that no deletions overlap with other deletions or
        // inserts (can't insert in the middle of a deletion?, or
        // allow that?, no?)
        let mut last_modification: Option<&Modification> = None;
        for modification in &self.modifications {
            if let Some(last) = last_modification {
                if modification.start() < last.end() {
                    bail!(
                        "overlapping document modifications: \
                         {last:?} and {modification:?}"
                    )
                }
            }
            last_modification = Some(modification);
        }
        Ok(())
    }

    /// Write the resulting string
    pub fn write_to<O: Write>(&mut self, output: &mut O) -> Result<()> {
        self.sort_and_check_modifications()?;
        let bytes = self.string.as_bytes();
        let mut last_modification: Option<&Modification> = None;
        for modification in &self.modifications {
            let part = if let Some(last) = last_modification {
                &bytes[last.end()..modification.start()]
            } else {
                &bytes[0..modification.start()]
            };
            output.write_all(part)?;
            match modification {
                Modification::Delete(_) => (),
                Modification::Insert(_, val) => output.write_all(val.as_bytes())?,
            }
            last_modification = Some(modification);
        }
        let part = if let Some(last) = last_modification {
            &bytes[last.end()..]
        } else {
            bytes
        };
        output.write_all(part)?;
        Ok(())
    }

    /// Return the resulting string. Panics if the applied
    /// modifications use ranges that don't adhere to UTF-8
    /// boundaries.
    pub fn to_string(&mut self) -> Result<String> {
        let mut output = Vec::new();
        self.write_to(&mut output)?;
        Ok(String::from_utf8(output).expect("modification ranges are correct"))
    }

    /// Return the resulting string, and whether that string is
    /// different from the original.
    pub fn to_string_and_modified(&mut self) -> Result<(String, bool)> {
        let output = self.to_string()?;
        let modified = output != self.string;
        Ok((output, modified))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t_basics() -> Result<()> {
        let s = "Hi there";
        let mut doc = ModifiedDocument::new(s);
        assert_eq!(doc.to_string()?, "Hi there");
        doc.push(Modification::Insert(3, "all out ".into()));
        assert_eq!(doc.to_string()?, "Hi all out there");
        doc.push(Modification::Delete(0..3));
        assert_eq!(doc.to_string()?, "all out there");
        doc.push(Modification::Delete(3..4));
        doc.push(Modification::Delete(7..s.len()));
        doc.push(Modification::Insert(s.len(), "s".into()));
        assert_eq!(doc.to_string()?, "all out hers");

        doc.push(Modification::Delete(6..s.len()));
        assert_eq!(
            doc.to_string().err().unwrap().to_string(),
            "overlapping document modifications: Delete(6..8) and Delete(7..8)"
        );
        Ok(())
    }

    #[test]
    fn t_more_overlapping() -> Result<()> {
        let s = "Hi there";

        let mut doc = ModifiedDocument::new(s);
        doc.push(Modification::Delete(3..5));
        doc.push(Modification::Insert(3, "H".into()));
        assert_eq!(doc.to_string()?, "Hi Here");

        let mut doc = ModifiedDocument::new(s);
        doc.push(Modification::Delete(3..5));
        doc.push(Modification::Insert(4, "H".into()));
        assert_eq!(
            doc.to_string().err().unwrap().to_string(),
            "overlapping document modifications: Delete(3..5) and Insert(4, \"H\")"
        );

        Ok(())
    }
}
