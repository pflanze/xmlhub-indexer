use std::{
    ffi::OsStr,
    fmt::{Debug, Display},
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, Context, Result};
use run_git::command::{run, Capturing};
use toml::Value;

use crate::installation::binaries_repo;

pub fn check_cargo_toml_no_path<P: AsRef<Path> + Debug>(cargo_toml_path: P) -> Result<()> {
    (|| -> Result<()> {
        let string =
            std::fs::read_to_string(&cargo_toml_path).with_context(|| anyhow!("reading file"))?;
        let val: Value = string.parse()?;
        let top = val
            .as_table()
            .ok_or_else(|| anyhow!("expecting table at the top level"))?;

        let mut bad = Vec::new();
        // Hmm, is `dependencies` actually optional?
        // XX todo: also check [patch.crates-io] but that is nested.
        for (section_name, required) in [("dependencies", false), ("build-dependencies", false)] {
            let section = match top.get(section_name) {
                Some(val) => val,
                None => {
                    if required {
                        bail!("missing {section_name:?} section")
                    } else {
                        continue;
                    }
                }
            };

            let entries = section
                .as_table()
                .ok_or_else(|| anyhow!("expecting section {section_name:?} to be a table"))?;
            for (package_name, val) in entries {
                match val {
                    Value::Table(table) => {
                        if let Some(path) = table.get("path") {
                            let ok = if let Some(s) = path.as_str() {
                                s.starts_with("libs/")
                            } else {
                                false
                            };
                            if !ok {
                                bad.push((section_name, package_name, path));
                            }
                        }
                    }
                    Value::String(_) => (),
                    _ => bail!(
                        "expecting package entry for dependencies to be a table or string, \
                         but for {package_name:?} got: {val:?}"
                    ),
                }
            }
        }
        if !bad.is_empty() {
            bail!(
                "the file has the following package entries with `path` entries, \
                 (section_name, package_name, path)--those \
                 would not build for other people who do not have the right source \
                 checked out in the right places: {bad:?}"
            )
        }
        Ok(())
    })()
    .with_context(|| anyhow!("checking Cargo toml file {cargo_toml_path:?} for `path =` entries"))
}

#[derive(Debug, Clone, Copy)]
pub enum CompilationProfile {
    Debug,
    Release,
}

impl Display for CompilationProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            CompilationProfile::Debug => "debug",
            CompilationProfile::Release => "release",
        };
        f.write_str(name)
    }
}

impl CompilationProfile {
    pub fn as_option_str(&self) -> &'static str {
        match self {
            CompilationProfile::Debug => "--debug",
            CompilationProfile::Release => "--release",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Env {
    None,
    Gnu,
    Musl,
    // Msvc,
    // Sgx
}

impl Env {
    pub fn as_str_for_target_triple(self) -> &'static str {
        match self {
            Env::None => "",
            Env::Gnu => "-gnu",
            Env::Musl => "-musl",
        }
    }
}

/// Representation of e.g. "aarch64-apple-darwin"
#[derive(Debug, Clone)]
pub struct TargetTriple {
    pub arch: binaries_repo::Arch,
    pub os: binaries_repo::Os,
    pub env: Env,
}

impl Display for TargetTriple {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self { arch, os, env } = self;
        f.write_fmt(format_args!(
            "{}-{}{}",
            arch.as_str_for_target_triple(),
            os.as_str_for_target_triple(),
            env.as_str_for_target_triple(),
        ))
    }
}

pub fn run_cargo<P: AsRef<Path>, S: AsRef<OsStr> + Debug>(
    working_dir: P,
    args: &[S],
) -> Result<()> {
    run(working_dir, "cargo", args, &[], &[0], Capturing::none())?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct CompilationTarget {
    pub target_triple: Option<TargetTriple>,
    pub profile: CompilationProfile,
}

impl Display for CompilationTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self {
            target_triple,
            profile,
        } = self;
        let target = target_triple
            .as_ref()
            .map(|v| format!("architecture/OS {v}"))
            .unwrap_or_else(|| String::from("host architecture/OS"));
        write!(f, "the {target} in {profile} mode")
    }
}

impl CompilationTarget {
    /// The path to the compiled binary of the main program, relative
    /// from the source repository base.
    pub fn subpath_to_binary(&self, program_name: &str) -> PathBuf {
        let Self {
            target_triple,
            profile,
        } = self;
        if let Some(target_triple) = target_triple {
            format!("target/{target_triple}/{profile}/{program_name}")
        } else {
            format!("target/{profile}/{program_name}")
        }
        .into()
    }

    pub fn run_build_in<P: AsRef<Path>>(&self, working_dir: P, program_name: &str) -> Result<()> {
        let mut args: Vec<String> = vec![
            "build".into(),
            self.profile.as_option_str().into(),
            "--bin".into(),
            program_name.into(),
        ];
        if let Some(target_triple) = &self.target_triple {
            args.push("--target".into());
            args.push(target_triple.to_string());
        }
        run_cargo(working_dir, &args)
    }
}
