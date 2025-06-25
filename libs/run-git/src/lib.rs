pub mod base_and_rel_path;
pub mod command;
pub mod flattened;
pub mod git;
pub mod path_util;
pub mod util;

/// In situation requiring an array or slice with a generic type for
/// the items, passing the empty array or slice is a bit cumbersome,
/// e.g. `&[] as &[&str]`. This macro may or may not make this nicer.
#[macro_export]
macro_rules! empty {
    [$T:ty] => {
        {&[] as &[$T]}
    }
}
