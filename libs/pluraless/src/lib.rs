//! Simple library to handle English pluralization.

// Re-export the identifiers in the `pluraless` namespace so that it
// is enough for the user to depend on `pluraless`. And we need to
// re-export everything *here* (and depend ourselves on the
// dependencies of `pluraless-macro`, i.e. `pluraless-impl`), because
// proc-macro crates "currently" cannot do re-exports of anything
// other than proc-macros. Uh.

pub use pluraless_impl::PluralizedWord;
pub use pluraless_macro::pluralized_let;

/// `pluralized!{n => theses, these}` binds the variable `theses` to
/// `"thesis"` if n is 1, or `"theses"` otherwise, and likewise the
/// variable `these` to `"this"` or `"these"`.
#[macro_export]
macro_rules! pluralized {
    { $n:expr => $id:ident } => {
        $crate::pluralized_let!{let $id = $n;}
    };
    { $n:expr => $id:ident, $($rest:tt)* } => {
        pluralized!{ $n => $id }
        pluralized!{ $n => $($rest)* }
    }
}
