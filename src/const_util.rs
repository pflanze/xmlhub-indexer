//! Utility functions that work in a const context.

/// A simple function to get the last segment from a unix style path
/// string when split on '/'. Returns path if it doesn't contain a
/// '/'. Careful, if path is "foo/.." then it will return "..", for
/// example.
pub const fn file_name(path: &str) -> &str {
    if path.is_empty() {
        return path;
    }
    let bytes = path.as_bytes();
    let mut i = bytes.len() - 1;
    loop {
        if bytes[i] == b'/' {
            let ptr_after = unsafe { bytes.as_ptr().add(i + 1) };
            let len = bytes.len() - (i + 1);
            let slice = unsafe { std::slice::from_raw_parts(ptr_after, len) };
            return unsafe { std::str::from_utf8_unchecked(slice) };
        }
        if i == 0 {
            return path;
        }
        i = i - 1;
    }
}

#[cfg(test)]
#[test]
fn t_() {
    let t = file_name;
    assert_eq!(t(""), "");
    assert_eq!(t("."), ".");
    assert_eq!(t("foo"), "foo");
    assert_eq!(t("foo/"), "");
    assert_eq!(t("foo/a"), "a");
    // and, well, it's simple we said
    assert_eq!(t("foo/.."), "..");
    assert_eq!(t("/.."), "..");
}
