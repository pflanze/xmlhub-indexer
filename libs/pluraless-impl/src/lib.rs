//! Simple library to handle English pluralization. See the
//! `pluraless-macro` crate for ergonomic use.

const SPECIAL: &[(&str, &str)] = &[
    // (plural, singular)
    ("these", "this"),
    ("are", "is"),
    ("theses", "thesis"),
    // ("", ""),
    // ("they", "it"), -- m, f, ?
];

/// Representation of a single word in plural/singular form.
#[derive(Debug, PartialEq)]
pub struct PluralizedWord<'s> {
    pub plural: &'s str,
    pub singular: &'s str,
}

impl<'s> PluralizedWord<'s> {
    pub fn n<N: Numeric>(&self, n: N) -> &'s str {
        if n.is_plural() {
            self.plural
        } else {
            self.singular
        }
    }
}

pub trait Numeric {
    fn is_plural(self) -> bool;
}

macro_rules! defnumeric {
    { $t:ty, $one:expr } => {
        impl Numeric for $t {
            fn is_plural(self) -> bool {
                if self == $one {
                    false
                } else {
                    true
                }
            }
        }
    }
}

defnumeric! {f64, 1.}
defnumeric! {f32, 1.}

defnumeric! {usize, 1}
defnumeric! {u64, 1}
defnumeric! {u32, 1}
defnumeric! {u16, 1}
defnumeric! {u8, 1}

defnumeric! {isize, 1}
defnumeric! {i64, 1}
defnumeric! {i32, 1}
defnumeric! {i16, 1}
defnumeric! {i8, 1}

fn special(plural: &str) -> Option<PluralizedWord> {
    SPECIAL
        .into_iter()
        .find(|(p, _)| *p == plural)
        .map(|(plural, singular)| PluralizedWord { plural, singular })
}

/// `word` should be an English word in the plural form. Returns
/// `None` if it does not know how to pluralize.
pub fn english_plural(word_in_plural: &str) -> Option<PluralizedWord> {
    if let Some(pl) = special(word_in_plural) {
        return Some(pl);
    }

    if word_in_plural.ends_with('s') {
        return Some(PluralizedWord {
            plural: word_in_plural,
            singular: &word_in_plural[0..word_in_plural.len() - 1],
        });
    }

    None
}

/// Same as `english_plural` but panics when it doesn't know the
/// answer; useful when you want to panic anyway (e.g. at compile
/// time) to have the message/backtrace point to this crate, not the
/// user code.
pub fn xenglish_plural(word_in_plural: &str) -> PluralizedWord {
    if let Some(pl) = english_plural(word_in_plural) {
        return pl;
    }
    panic!(
        "pluraless:english_plural: the word {word_in_plural:?} is not in the \
         list of special cases and is not ending in 's', please adapt the list"
    )
}

#[test]
fn t_english_plural() {
    let t = |n, s| {
        let w = xenglish_plural(s);
        w.n(n)
    };
    assert_eq!(t(0, "fields"), "fields");
    assert_eq!(t(1, "fields"), "field");
    assert_eq!(t(2, "fields"), "fields");
    assert_eq!(t(2, "these"), "these");
    assert_eq!(t(0, "these"), "these");
    assert_eq!(t(1, "these"), "this");
    assert_eq!(
        english_plural("these"),
        Some(PluralizedWord {
            plural: "these",
            singular: "this"
        })
    );
    assert_eq!(
        english_plural("theses"),
        Some(PluralizedWord {
            plural: "theses",
            singular: "thesis"
        })
    );
    assert_eq!(
        english_plural("theseres"),
        Some(PluralizedWord {
            plural: "theseres",
            singular: "thesere"
        })
    );
    assert_eq!(english_plural("thesem"), None);
}
