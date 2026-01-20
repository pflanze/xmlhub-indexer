use std::{ffi::CString, fmt::Debug};

use anyhow::{anyhow, Context, Result};

pub fn cstring<T: AsRef<[u8]> + Debug>(s: T) -> Result<CString> {
    CString::new(s.as_ref()).with_context(|| anyhow!("converting string-like to C string: {s:?}"))
}
