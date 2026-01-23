use std::time::SystemTime;

use chrono::{DateTime, Local, Utc};

#[derive(Debug, Clone, clap::Args)]
pub struct TimestampFormatter {
    /// Whether rfc3339 format is to be used (default: whatever chrono
    /// uses for `Display`).
    #[clap(long)]
    pub use_rfc3339: bool,

    /// If true, write log time stamps in the local time zone.
    /// Default: in UTC.
    #[clap(long)]
    pub local_time: bool,
}

impl TimestampFormatter {
    pub fn format_systemtime(&self, t: SystemTime) -> String {
        let Self {
            use_rfc3339,
            local_time,
        } = self;
        if *local_time {
            let t: DateTime<Local> = t.into();
            if *use_rfc3339 {
                t.to_rfc3339()
            } else {
                t.to_string()
            }
        } else {
            let t: DateTime<Utc> = t.into();
            if *use_rfc3339 {
                t.to_rfc3339()
            } else {
                t.to_string()
            }
        }
    }
}
