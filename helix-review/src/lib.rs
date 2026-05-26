//! Local code review types, persistence, and LLM formatting.

mod context;
mod diff_map;
mod format;
mod ids;
mod map;
mod paths;
mod storage;
mod types;

pub use context::capture_line_context;
pub use diff_map::{parse_unified_diff_line_map, DiffLineMapping, DiffReviewSource};
pub use format::format_review_for_llm;
pub use ids::{new_comment_id, timestamp_now};
pub use map::{map_comments_for_changes, map_session_comments_for_file};
pub use paths::{normalize_path, paths_equal, repo_slug, review_dir, reviews_root};
pub use storage::{create_new_review, list_reviews, load_review, save_review, ReviewListEntry};
pub use types::{
    DiffSide, ReviewComment, ReviewData, ReviewMetadata, ReviewStatus,
};
