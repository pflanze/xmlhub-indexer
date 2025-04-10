use std::io::{stderr, Write};

// Helpers for the varous check_dry_run macros. Do not actually use
// `eprintln!` since that can panic.

pub fn eprintln_dry_run(s: String) {
    _ = writeln!(&mut stderr(), "+ --dry-run: would run: {s}");
}

pub fn eprintln_running(s: String) {
    _ = writeln!(&mut stderr(), "+ running: {s}");
}
