//! File utils that use a trash can.

use std::path::Path;

use anyhow::{anyhow, Context, Result};

pub fn write_file_moving_to_trash_if_exists(
    target_path: &Path,
    content: &str,
    quiet: bool,
) -> Result<()> {
    if target_path.exists() {
        trash::delete(&target_path)
            .with_context(|| anyhow!("moving existing target file {target_path:?} to trash"))?;
        if !quiet {
            println!("Moved existing target file {target_path:?} to trash.");
        }
    }
    std::fs::write(&target_path, content)
        .with_context(|| anyhow!("writing contents to file {target_path:?}"))?;
    Ok(())
}
