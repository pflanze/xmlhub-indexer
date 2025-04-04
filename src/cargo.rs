use std::{fmt::Debug, path::Path};

use anyhow::{anyhow, bail, Context, Result};
use toml::Value;

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
