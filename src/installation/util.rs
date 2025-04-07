use anyhow::{Context, Result};
use chrono::Local;

use crate::util::hostname;

/// Current time in rfc2822 format.
pub fn get_timestamp() -> String {
    Local::now().to_rfc2822()
}

/// Get user@hostname
pub fn get_creator() -> Result<String> {
    let username = std::env::var("USER").context("retrieving USER environment variable")?;
    let hostname = hostname()?;
    Ok(format!("{username}@{hostname}"))
}
