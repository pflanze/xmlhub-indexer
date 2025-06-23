
/// Does the same for bytes that `haystack.contains(needle)` does for
/// strings. (This will be in std in the future:
/// <https://github.com/rust-lang/rust/issues/134149>)
pub fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}
