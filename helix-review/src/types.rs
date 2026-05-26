use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Side of a diff hunk a comment applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffSide {
    Unchanged,
    Added,
    Modified,
    Removed,
}

/// Lifecycle status of a review session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReviewStatus {
    Draft,
    Submitted,
}

/// A single line-anchored review comment with snapshot context for LLM use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewComment {
    pub id: String,
    pub file: PathBuf,
    pub line: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_end: Option<usize>,
    #[serde(skip)]
    pub char_idx: usize,
    pub body: String,
    pub context_before: Vec<String>,
    pub context_after: Vec<String>,
    pub code_at_comment: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff_side: Option<DiffSide>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hunk_index: Option<usize>,
    pub created_at: String,
}

/// Review session metadata stored separately on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewMetadata {
    pub id: String,
    pub repo_root: PathBuf,
    pub title: String,
    pub status: ReviewStatus,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submitted_at: Option<String>,
    #[serde(default)]
    pub summary: String,
}

/// Full review payload (metadata + comments).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewData {
    pub metadata: ReviewMetadata,
    pub comments: Vec<ReviewComment>,
}
