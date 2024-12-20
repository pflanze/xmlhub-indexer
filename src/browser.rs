use std::{env, ffi::OsStr, path::Path, process::Child};

use anyhow::{bail, Result};

use crate::{command::spawn, util::flatten};

/// Find a web browser and run it with the given arguments. Tries
/// `sensible-browser`, the browsers specified in the `BROWSER`
/// environment variable, which is split on ':' into program names or
/// paths that are tried in order, `firefox`, `chromium` or
/// `chrome`. Fails if none worked.
pub fn spawn_browser(in_directory: &Path, arguments: &[&OsStr]) -> Result<Child> {
    let prolonged_ownership_val;
    let env_browsers: Vec<&str> = match env::var("BROWSER") {
        Ok(val) => {
            prolonged_ownership_val = val;
            prolonged_ownership_val.split(':').collect()
        }
        Err(e) => match e {
            env::VarError::NotPresent => vec![],
            env::VarError::NotUnicode(_) => bail!("reading BROWSER env var: {e}"),
        },
    };
    let choices = flatten([
        // Due to using spawn, we stop the search if
        // "sensible-browser" (a Debian thing) is found; thus we
        // try that first, then if not available, try the BROWSER
        // items. "sensible-browser" also tries the BROWSER
        // entries, so it would be duplication here, but we know
        // at that point "sensible-browser" hasn't tried because
        // it wasn't executed.
        ["sensible-browser"].as_ref(),
        env_browsers.as_ref(),
        ["firefox", "chromium", "chrome"].as_ref(),
    ]);
    let mut errors = Vec::new();
    for browser in &choices {
        match spawn(in_directory, *browser, arguments, &[]) {
            Ok(handle) => return Ok(handle),
            Err(e) => errors.push(format!("{e:#}")),
        }
    }
    bail!("could not find a web browser, tried: {choices:?} (errors: {errors:?})")
}
