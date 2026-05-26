use std::path::{Path, PathBuf};

use helix_loader::data_dir;

/// Returns the basename of the current workspace for review storage paths.
pub fn repo_slug() -> String {
    let (workspace, _) = helix_loader::find_workspace();
    workspace
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown")
        .to_string()
}

/// Root directory for all reviews: `data_dir()/reviews`.
pub fn reviews_root() -> PathBuf {
    data_dir().join("reviews")
}

/// Directory for a specific review session.
pub fn review_dir(repo_slug: &str, review_id: &str) -> PathBuf {
    reviews_root().join(repo_slug).join(review_id)
}

/// Normalize a path for stable comparison within a repository.
pub fn normalize_path(path: &Path, repo_root: &Path) -> PathBuf {
    let canonical = helix_stdx::path::canonicalize(path);
    let repo_canonical = helix_stdx::path::canonicalize(repo_root);
    if let Ok(stripped) = canonical.strip_prefix(&repo_canonical) {
        return stripped.to_path_buf();
    }
    canonical
}

/// Returns true when two paths refer to the same file within a repository.
pub fn paths_equal(a: &Path, b: &Path, repo_root: &Path) -> bool {
    normalize_path(a, repo_root) == normalize_path(b, repo_root)
}
