//! Various types used by xmlhub-indexer

#[derive(Clone)]
pub struct OutputFile {
    /// Relative path from the top of the xmlhub repository
    pub path_from_repo_top: &'static str,
}
