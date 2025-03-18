use crate::checkout_context::CheckoutContext;

/// The relative path from `SOURCE_CHECKOUT.working_dir_path` to the
/// compiled binary of the main program.
pub const XMLHUB_INDEXER_BINARY_FILE: &str = "target/release/xmlhub";

/// Information on the Git checkout of the xmlhub-indexer repo; used
/// by both xmlhub.rs and make-xmlhub-indexer-release.rs
pub const SOURCE_CHECKOUT: CheckoutContext<&str> = CheckoutContext {
    // This path is only used by make-xmlhub-indexer-release.rs and
    // replaced with the program argument for xmlhub.rs
    working_dir_path: ".",
    branch_name: "master",
    supposed_upstream_git_url: "git@cevo-git.ethz.ch:cevo-resources/xmlhub-indexer.git",
    supposed_upstream_web_url: "https://cevo-git.ethz.ch/cevo-resources/xmlhub-indexer",
};

/// Information on the Git checkout of the xmlhub-indexer-binaries repo; currently used
/// only by make-xmlhub-indexer-release.rs
pub const BINARIES_CHECKOUT: CheckoutContext<&str> = CheckoutContext {
    working_dir_path: "../xmlhub-indexer-binaries/",
    branch_name: "master",
    supposed_upstream_git_url: "git@cevo-git.ethz.ch:cevo-resources/xmlhub-indexer-binaries.git",
    supposed_upstream_web_url: "https://cevo-git.ethz.ch/cevo-resources/xmlhub-indexer-binaries",
};
