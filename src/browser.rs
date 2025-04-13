use std::{
    env,
    ffi::{OsStr, OsString},
    ops::Deref,
    path::Path,
    process::Child,
};

use anyhow::{anyhow, bail, Context, Result};

use crate::{
    command::{run_outputs, spawn, Capturing},
    path_util::CURRENT_DIRECTORY,
    util::{append, to_owned_items},
};

const LINUX_BROWSERS: &[&str] = &["sensible-browser", "firefox", "chromium", "chrome"];

enum BrowsersSource {
    Env,
    HardCoded,
}

impl BrowsersSource {
    fn to_str(self) -> &'static str {
        match self {
            BrowsersSource::Env => "BROWSER env variable",
            BrowsersSource::HardCoded => "hard coded browsers list",
        }
    }
}

fn get_browsers() -> Result<(BrowsersSource, Vec<String>)> {
    let linux_browsers = || {
        (
            BrowsersSource::HardCoded,
            LINUX_BROWSERS
                .iter()
                .map(Deref::deref)
                .map(ToOwned::to_owned)
                .collect(),
        )
    };
    match env::var("BROWSER") {
        Ok(val) => {
            let bs: Vec<_> = val
                .split(':')
                .filter(|s| !s.is_empty())
                .map(ToOwned::to_owned)
                .collect();
            Ok(if bs.is_empty() {
                linux_browsers()
            } else {
                (BrowsersSource::Env, bs)
            })
        }
        Err(e) => match e {
            env::VarError::NotPresent => Ok(linux_browsers()),
            env::VarError::NotUnicode(_) => bail!("reading BROWSER env var: {e}"),
        },
    }
}

pub fn spawn_browser_linux(in_directory: &Path, arguments: &[&OsStr]) -> Result<Child> {
    let (browsers_source, browsers) = get_browsers()?;

    let mut errors = Vec::new();
    for browser in &browsers {
        match spawn(in_directory, browser, arguments, &[], Capturing::none()) {
            Ok(handle) => return Ok(handle),
            // I wish I could split the anyhow into separate parts,
            // increasingly indented, but "{e:#}" is the best we can
            // do here (short of writing a custom formatter?)
            Err(e) => errors.push(format!("{e:#}")),
        }
    }
    bail!(
        "could not find a web browser, tried {browsers:?} from {}:\n{}",
        browsers_source.to_str(),
        errors
            .iter()
            .map(|e| format!("\t{}", e.trim_end()))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

fn spawn_browser_macos(in_directory: &Path, arguments: &[&OsStr]) -> Result<()> {
    let (browsers_source, mut browsers) = get_browsers()?;
    match browsers_source {
        BrowsersSource::Env => (),
        BrowsersSource::HardCoded => {
            // First try the default action? But that will presumably
            // always succeed, but "worse", that may be a text editor
            // or other application. Thus instead try the normal
            // alternative browser names first, then also Safari--ah,
            // and Chrome is called Google Chrome on macOS:
            browsers.push("google chrome".into());
            browsers.push("safari".into());
        }
    }

    let mut errors = Vec::new();
    for browser in &browsers {
        let all_arguments = append(
            [OsString::from("-a"), OsString::from(browser)],
            to_owned_items(arguments),
        );

        let may_be_gui_program_name = match &browsers_source {
            BrowsersSource::Env => !browser.contains('/'),
            BrowsersSource::HardCoded => true,
        };

        if may_be_gui_program_name {
            let mut outputs = run_outputs(in_directory, "open", &all_arguments, &[], &[0, 1])
                .with_context(|| {
                    anyhow!("starting a browser, trying 'open' with argument {all_arguments:?}")
                })?;
            if outputs.truthy {
                return Ok(());
            }
            outputs.indent = "\t\t";
            errors.push(format!(
                "* {browser:?} failed executed via open -a:\n\t\t{outputs}",
            ));
        }

        // Try as path or program name via $PATH instead
        match spawn(in_directory, browser, arguments, &[], Capturing::none()) {
            Ok(_handle) => return Ok(()),
            Err(e) => errors.push(format!("* {browser:?} failed executed directly: {e:#}")),
        }
    }
    bail!(
        "could not find a web browser, tried {browsers:?} from {}:\n{}",
        browsers_source.to_str(),
        errors
            .iter()
            .map(|e| format!("\t{}", e.trim_end()))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

/// Find a web browser and run it with the given arguments. If the
/// `BROWSER` environment variable is set, splits it on ':' into
/// browser names or paths (when containing at least one '/') and
/// tries executing those. Otherwise tries "sensible-browser",
/// "firefox", "chromium", "chrome" in turn.  Fails if none could be
/// started or an env variable could not be decoded as UTF-8. On
/// macOS, browser names are opened via `open -a`, paths directly (but
/// note that passing a path to an executable in
/// `/Applications/$appname.app/..somewhere..` may ignore arguments,
/// instead use just $appname).
pub fn spawn_browser(in_directory: &Path, arguments: &[&OsStr]) -> Result<()> {
    match std::env::consts::OS {
        "macos" => spawn_browser_macos(in_directory, arguments),
        _ => match std::env::consts::FAMILY {
            "unix" => {
                spawn_browser_linux(in_directory, arguments)?;
                Ok(())
            }
            // "windows" =>
            s => bail!("spawn_browser: don't know how to handle OS family {s:?}"),
        },
    }
}

/// Simplified call to just open a local filesystem path in the
/// browser (absolute or relative to the current directory).
pub fn spawn_browser_on_path(document_path: &Path) -> Result<()> {
    spawn_browser(*CURRENT_DIRECTORY, &[&OsString::try_from(document_path)?])?;
    Ok(())
}
