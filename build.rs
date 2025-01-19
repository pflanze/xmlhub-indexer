use std::path::PathBuf;
use std::process::Command;

fn main() {
    // Pass `git describe --tags` to programs to be picked up via
    // `env!("GIT_DESCRIBE")`.
    let args = include!("include/git_describe_arguments.rs");
    let output = Command::new("git")
        .args(args)
        .output()
        .expect("`git` command should be available and not fail to run {args:?}");

    let stdout = String::from_utf8(output.stdout).expect("git describe should print utf-8");
    let version = stdout.trim();

    println!("cargo:rustc-env=GIT_DESCRIBE={}", version);

    // build.rs is only re-run when an actual build happens; potential
    // changes to the git describe output above are hence not taken by
    // cargo as inputs for rebuild decisions. Instead, have to declare
    // dependencies via rerun-if-changed, but that can only be on env
    // vars (*outside* build.rs) or files or dirs. Hence:

    // Rebuild if make-xmlhub-indexer-release creates/updates this file, which it
    // does when it creates a tag. But also create the file right now
    // if it doesn't already exist, since if it's not there, cargo
    // will rebuild the binary every time it is invoked!
    let path_str = ".released_version";
    let path = PathBuf::from(path_str);
    if !path.exists() {
        std::fs::write(path, version).expect("path should be writable: {path:?}");
    }
    println!("cargo::rerun-if-changed={path_str}");

    // Also try to detect when Git has a new tag, this should take
    // care of the case where somebody makes a new tag manually, or
    // just a new commit (which is definitely bypassing
    // make-xmlhub-indexer-release). The drawback of this is that when Git changes
    // the layout of its metadata dir then this will stop working (and
    // already today, this will fail if packed refs are used).
    println!("cargo::rerun-if-changed=.git/logs/HEAD");
    println!("cargo::rerun-if-changed=.git/refs/tags/");
}
