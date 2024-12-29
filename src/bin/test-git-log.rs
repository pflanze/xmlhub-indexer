use std::path::PathBuf;

use anyhow::Result;

use xmlhub_indexer::git::git_log;

fn main() -> Result<()> {
    {
        for (i, entry) in git_log(&PathBuf::from("."), &["--"])?.enumerate() {
            let entry = entry?;
            println!("{i}. {entry:#?}");
            if i >= 5 {
                break;
            }
        }
    }
    println!("Ok.");
    Ok(())
}
