//! `xmlhub_indexer_defaults` was supposed to be xmlhub specific but
//! is also covering application upgrades now. (TODO: clean up)
use crate::checkout_context::CheckoutContext;

pub const XMLHUB_BINARY_FILE_NAME: &str = "xmlhub";

/// Information on the Git checkout of the xmlhub repo; used
/// by xmlhub.rs
pub const XMLHUB_CHECKOUT: CheckoutContext<&str> = CheckoutContext {
    // This path is replaced with the BASE_PATH argument
    working_dir_path: ".",
    branch_name: "master",
    supposed_upstream_git_url: "git@cevo-git.ethz.ch:cevo-resources/xmlhub.git",
    supposed_upstream_web_url: "https://cevo-git.ethz.ch/cevo-resources/xmlhub",
    expected_sub_paths: &["attributes.md"],
};

/// Information on the Git checkout of the xmlhub-indexer repo; used
/// by both xmlhub.rs and make-release.rs
pub const SOURCE_CHECKOUT: CheckoutContext<&str> = CheckoutContext {
    // This path is only used by make-release.rs and
    // replaced with the program argument for xmlhub.rs
    working_dir_path: ".",
    branch_name: "master",
    supposed_upstream_git_url: "git@cevo-git.ethz.ch:cevo-resources/xmlhub-indexer.git",
    supposed_upstream_web_url: "https://cevo-git.ethz.ch/cevo-resources/xmlhub-indexer",
    expected_sub_paths: &["Cargo.toml", "src/bin/xmlhub.rs"],
};

/// Information on the Git checkout of the xmlhub-indexer-binaries repo; currently used
/// only by make-release.rs
pub const BINARIES_CHECKOUT: CheckoutContext<&str> = CheckoutContext {
    working_dir_path: "../xmlhub-indexer-binaries/",
    branch_name: "master",
    supposed_upstream_git_url: "git@cevo-git.ethz.ch:cevo-resources/xmlhub-indexer-binaries.git",
    supposed_upstream_web_url: "https://cevo-git.ethz.ch/cevo-resources/xmlhub-indexer-binaries",
    expected_sub_paths: &["macOS", "keys"],
};
