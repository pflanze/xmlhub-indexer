use std::{fs::File, io::Read, path::Path};

use anyhow::Result;
use run_git::command::run_stdout_string;
use sha2::{Digest, Sha256};

use crate::{fixup_path::CURRENT_DIRECTORY, rayon_util::ParRun, utillib::hex::to_hex_string};

/// Calculate SHA-256 hash sum for the given path as hex string.
pub fn sha256sum<P: AsRef<Path>>(path: P) -> Result<String, std::io::Error> {
    // `Read` trait: buffer needs to be 'full' already, like in C,
    // &mut Vec silently doesn't read anything. Then have to take a
    // subslice of length n to feed to the hasher (`read` could return
    // that?). The `GenericArray` returned by the hasher can only be
    // turned into a slice via `AsRef`? Then use my own hex string
    // function. Odd that nothing is provided, am I missing some uber
    // hashing crate?
    const BUF_SIZE: usize = 1024 * 32;
    let mut hasher = Sha256::new();
    let mut input = File::open(path)?;
    let mut buffer: [u8; BUF_SIZE] = [0; BUF_SIZE];
    loop {
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    let result = hasher.finalize();
    let bytes: &[u8] = result.as_ref();
    Ok(to_hex_string(bytes))
}

/// Calculate SHA-256 hash sum for the given path as hex string, by
/// calling the external `sha256sum` executable.
pub fn sha256sum_external<P: AsRef<Path>>(path: P) -> Result<String> {
    let stdout = run_stdout_string(
        *CURRENT_DIRECTORY,
        "sha256sum",
        &[path.as_ref()],
        &[],
        &[0],
        true,
    )?;
    if let Some(pos) = stdout.find(|c: char| c.is_whitespace()) {
        Ok(stdout[0..pos].to_string())
    } else {
        Ok(stdout)
    }
}

/// Calculate SHA-256 hash sum for the given path as hex string, by
/// using the `sha2` crate and by calling the external `sha256sum`
/// executable and asserting that the results are the same.
pub fn sha256sum_paranoid<P: AsRef<Path> + Sync>(path: P) -> Result<String> {
    let (external, internal) = (
        || sha256sum_external(path.as_ref()),
        || sha256sum(path.as_ref()),
    )
        .par_run();
    let external = external?;
    let internal = internal?;
    assert_eq!(external, internal);
    Ok(external)
}
