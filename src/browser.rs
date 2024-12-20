use std::{
    env,
    ffi::{OsStr, OsString},
    ops::Deref,
    path::Path,
    process::Child,
};

use anyhow::{anyhow, bail, Context, Result};

use crate::{
    command::{run, spawn},
    util::append,
};

fn to_owned_items<O, T: ToOwned<Owned = O> + ?Sized>(vals: &[&T]) -> Vec<O> {
    vals.iter().map(|s| (*s).to_owned()).collect::<Vec<O>>()
}

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
        match spawn(in_directory, browser, arguments, &[]) {
            Ok(handle) => return Ok(handle),
            Err(e) => errors.push(format!("{e:#}")),
        }
    }
    bail!(
        "could not find a web browser, tried {browsers:?} from {}:\n{}",
        browsers_source.to_str(),
        errors
            .iter()
            .map(|e| format!("\t{e}"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

/// Find a web browser and run it with the given arguments, working on
/// MacOS. Tries a few other browsers than Safari first.
pub fn spawn_browser_macos(in_directory: &Path, arguments: &[&OsStr]) -> Result<()> {
    let (browsers_source, mut browsers) = get_browsers()?;
    match browsers_source {
        BrowsersSource::Env => (),
        BrowsersSource::HardCoded => {
            // First try the default action? But that will presumably
            // always succeed, but "worse", that may be a text editor
            // or so. Thus instead try the normal alternative browser
            // names first, then also Safari:
            browsers.push("safari".into());
        }
    }

    let mut errors = Vec::new();
    for browser in &browsers {
        let all_arguments = append(
            &[OsString::from("-a"), OsString::from(browser)],
            &to_owned_items(arguments),
        );

        let may_be_gui_program_name = match &browsers_source {
            BrowsersSource::Env => !browser.contains('/'),
            BrowsersSource::HardCoded => true,
        };

        if may_be_gui_program_name {
            if run(in_directory, "open", &all_arguments, &[], &[0, 1]).with_context(|| {
                anyhow!("starting a browser, trying 'open' with argument {all_arguments:?}")
            })? {
                return Ok(());
            }
            errors.push(format!("not found when executed via open -a"));
        }

        // Try as path or program name via $PATH instead
        match spawn(in_directory, browser, arguments, &[]) {
            Ok(_handle) => return Ok(()),
            Err(e) => errors.push(format!("error when executed directly: {e:#}")),
        }
    }
    bail!(
        "could not find a web browser, tried {browsers:?} from {}:\n{}",
        browsers_source.to_str(),
        errors
            .iter()
            .map(|e| format!("\t{e}"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

/// Find a web browser and run it with the given arguments. If the
/// `BROWSER` environment variable is set, splits it on ':' into
/// browser names or paths and tries executing those (if names, on
/// MacOS via `open -a`). Otherwise tries "sensible-browser",
/// "firefox", "chromium", "chrome" in turn. Fails if none could be
/// started or an env variable could not be decoded as UTF-8.
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
