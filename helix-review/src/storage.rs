//! Review persistence under `data_dir()/reviews/<repo-slug>/<review-id>/`.

use std::fs;
use std::path::Path;

use crate::{
    ids::timestamp_now, paths::review_dir, paths::reviews_root, ReviewData, ReviewMetadata,
    ReviewStatus,
};

/// Summary entry for review list pickers.
#[derive(Debug, Clone)]
pub struct ReviewListEntry {
    pub id: String,
    pub title: String,
    pub status: ReviewStatus,
    pub comment_count: usize,
    pub created_at: String,
}

pub fn create_new_review(
    repo_root: &Path,
    repo_slug: &str,
    title: impl Into<String>,
) -> ReviewData {
    let id = timestamp_now();
    let created_at = id.clone();
    let review = ReviewData {
        metadata: ReviewMetadata {
            id: id.clone(),
            repo_root: repo_root.to_path_buf(),
            title: title.into(),
            status: ReviewStatus::Draft,
            created_at,
            submitted_at: None,
            summary: String::new(),
        },
        comments: Vec::new(),
    };
    if let Err(err) = save_review(repo_slug, &review) {
        log::error!("failed to save new review: {err:#}");
    }
    review
}

pub fn save_review(repo_slug: &str, review: &ReviewData) -> anyhow::Result<()> {
    let dir = review_dir(repo_slug, &review.metadata.id);
    fs::create_dir_all(&dir)?;

    let metadata_path = dir.join("metadata.json");
    let review_path = dir.join("review.json");

    fs::write(
        metadata_path,
        serde_json::to_string_pretty(&review.metadata)?,
    )?;
    fs::write(review_path, serde_json::to_string_pretty(review)?)?;
    Ok(())
}

pub fn load_review(repo_slug: &str, review_id: &str) -> anyhow::Result<ReviewData> {
    let review_path = review_dir(repo_slug, review_id).join("review.json");
    let contents = fs::read_to_string(review_path)?;
    Ok(serde_json::from_str(&contents)?)
}

pub fn list_reviews(repo_slug: &str) -> Vec<ReviewListEntry> {
    let repo_dir = reviews_root().join(repo_slug);
    let Ok(entries) = fs::read_dir(repo_dir) else {
        return Vec::new();
    };

    let mut reviews = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(review_id) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Ok(review) = load_review(repo_slug, review_id) else {
            continue;
        };
        reviews.push(ReviewListEntry {
            id: review.metadata.id.clone(),
            title: review.metadata.title.clone(),
            status: review.metadata.status,
            comment_count: review.comments.len(),
            created_at: review.metadata.created_at.clone(),
        });
    }

    reviews.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    reviews
}

/// Remove a saved review and all on-disk artifacts.
pub fn delete_review(repo_slug: &str, review_id: &str) -> anyhow::Result<()> {
    let dir = review_dir(repo_slug, review_id);
    if dir.exists() {
        fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tempfile::TempDir;

    use super::*;
    use crate::{DiffSide, ReviewComment};

    #[test]
    fn review_persistence_roundtrip() {
        let temp = TempDir::new().unwrap();
        std::env::set_var("XDG_DATA_HOME", temp.path());

        let repo_root = PathBuf::from("/tmp/helix-review-test");
        let slug = "helix-review-test";
        let mut review = create_new_review(&repo_root, slug, "Persistence test");
        review.comments.push(ReviewComment {
            id: "c1".into(),
            file: repo_root.join("src/main.rs"),
            line: 4,
            line_end: None,
            char_idx: 0,
            body: "Use a helper".into(),
            context_before: vec!["fn main() {".into()],
            context_after: vec!["}".into()],
            code_at_comment: "    println!(\"hi\");".into(),
            diff_side: Some(DiffSide::Added),
            hunk_index: Some(0),
            created_at: "1".into(),
            author: crate::CommentAuthor::User,
        });

        save_review(slug, &review).unwrap();
        let loaded = load_review(slug, &review.metadata.id).unwrap();
        assert_eq!(loaded.comments.len(), 1);
        assert_eq!(loaded.comments[0].body, "Use a helper");
        assert_eq!(loaded.comments[0].author, crate::CommentAuthor::User);
    }
}
