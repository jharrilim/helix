//! Runtime review UI state and document sync.

use std::path::{Path, PathBuf};

use helix_review::{map_session_comments_for_file, normalize_path, paths_equal};

use crate::Document;

pub const DRAFT_COMMENT_ID: &str = "__draft__";

/// In-progress comment shown below the cursor while the review prompt is open.
#[derive(Debug, Clone)]
pub struct PendingReviewComment {
    pub file: PathBuf,
    /// Line in the current buffer used for virtual-line placement.
    pub display_line: usize,
    /// Line stored in the review session (source line for diff buffers).
    pub source_line: usize,
    pub body: String,
}

/// Where keyboard focus lives while composing a review comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReviewDraftFocus {
    #[default]
    Prompt,
    Buttons(ReviewDraftButton),
}

/// Footer button selection while drafting a review comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewDraftButton {
    Save,
    Delete,
}

/// Runtime review UI state on the editor.
#[derive(Debug, Default)]
pub struct ReviewState {
    pub active: bool,
    pub current: Option<helix_review::ReviewData>,
    pub repo_slug: String,
    pub navigation_index: usize,
    pub pending_comment: Option<PendingReviewComment>,
    pub draft_focus: ReviewDraftFocus,
}

impl ReviewState {
    pub fn ensure_repo_slug(&mut self) {
        if self.repo_slug.is_empty() {
            self.repo_slug = helix_review::repo_slug();
        }
    }
}

/// Resolve diff metadata for a comment on a source file buffer.
pub fn diff_metadata_for_line(
    doc: &Document,
    line: u32,
) -> (Option<usize>, Option<helix_review::DiffSide>) {
    let Some(diff_handle) = doc.diff_handle() else {
        return (None, None);
    };
    let hunks = diff_handle.load();
    let Some(hunk_idx) = hunks.hunk_at(line, false) else {
        return (None, None);
    };
    let hunk = hunks.nth_hunk(hunk_idx);
    let side = if hunk.is_pure_insertion() {
        helix_review::DiffSide::Added
    } else if hunk.is_pure_removal() {
        helix_review::DiffSide::Removed
    } else {
        helix_review::DiffSide::Modified
    };
    (Some(hunk_idx as usize), Some(side))
}

/// Apply pending document edits to session comments before rebuilding display caches.
pub fn flush_pending_comment_remaps(editor: &mut crate::Editor) {
    let Some(review) = editor.review.current.as_mut() else {
        for doc in editor.documents.values_mut() {
            doc.review_pending_changes = None;
        }
        return;
    };

    let repo_root = review.metadata.repo_root.clone();
    let doc_ids: Vec<_> = editor.documents.keys().copied().collect();
    for doc_id in doc_ids {
        let Some(changes) = editor
            .documents
            .get_mut(&doc_id)
            .and_then(|doc| doc.review_pending_changes.take())
        else {
            continue;
        };
        let Some(doc) = editor.documents.get(&doc_id) else {
            continue;
        };
        let Some(path) = doc.path() else {
            continue;
        };
        let text = doc.text().clone();
        map_session_comments_for_file(
            &mut review.comments,
            path,
            &repo_root,
            &changes,
            &text,
        );
    }
}

/// Sync comments from the active review into a document's display cache.
pub fn sync_comments_to_document(review: &helix_review::ReviewData, doc: &mut Document) {
    doc.review_comments.clear();
    let repo_root = &review.metadata.repo_root;

    if let Some(diff_source) = &doc.diff_review_source {
        for (diff_line, mapping) in diff_source.line_map.iter().enumerate() {
            let Some(mapping) = mapping else { continue };
            for comment in &review.comments {
                if paths_equal(&comment.file, &mapping.source_path, repo_root)
                    && comment.line == mapping.source_line
                {
                    let mut display = comment.clone();
                    display.char_idx = doc.text().line_to_char(diff_line);
                    display.line = diff_line;
                    doc.review_comments.push(display);
                }
            }
        }
    } else if let Some(path) = doc.path().cloned() {
        for comment in &review.comments {
            if paths_equal(&comment.file, &path, repo_root) {
                let mut display = comment.clone();
                display.char_idx = doc.text().line_to_char(comment.line);
                doc.review_comments.push(display);
            }
        }
    }

    doc.review_comments.sort_by_key(|comment| (comment.line, comment.char_idx));
}

fn apply_pending_comment_to_document(
    pending: &PendingReviewComment,
    doc: &mut Document,
    repo_root: &Path,
) {
    let body = pending.body.clone();

    if let Some(diff_source) = &doc.diff_review_source {
        let Some(mapping) = diff_source.line_map.get(pending.display_line).and_then(|m| m.as_ref()) else {
            return;
        };
        if !paths_equal(&pending.file, &mapping.source_path, repo_root) {
            return;
        }
        let display = helix_review::ReviewComment {
            id: DRAFT_COMMENT_ID.into(),
            file: pending.file.clone(),
            line: pending.display_line,
            line_end: None,
            char_idx: doc.text().line_to_char(pending.display_line),
            body: body.clone(),
            author: helix_review::CommentAuthor::User,
            context_before: Vec::new(),
            context_after: Vec::new(),
            code_at_comment: String::new(),
            diff_side: None,
            hunk_index: None,
            created_at: String::new(),
        };
        doc.review_comments.push(display);
    } else if let Some(path) = doc.path().cloned() {
        if !paths_equal(&pending.file, &path, repo_root) {
            return;
        }
        let display = helix_review::ReviewComment {
            id: DRAFT_COMMENT_ID.into(),
            file: pending.file.clone(),
            line: pending.display_line,
            line_end: None,
            char_idx: doc.text().line_to_char(pending.display_line),
            body,
            author: helix_review::CommentAuthor::User,
            context_before: Vec::new(),
            context_after: Vec::new(),
            code_at_comment: String::new(),
            diff_side: None,
            hunk_index: None,
            created_at: String::new(),
        };
        doc.review_comments.push(display);
    }
}

/// Mirror draft UI state (prompt cursor, focused footer button) into open documents.
pub fn sync_review_draft_ui(
    editor: &mut crate::Editor,
    prompt_cursor: Option<usize>,
) {
    let (cursor, button) = match (
        editor.review.pending_comment.is_some(),
        editor.review.draft_focus,
    ) {
        (true, ReviewDraftFocus::Prompt) => (prompt_cursor, None),
        (true, ReviewDraftFocus::Buttons(button)) => (None, Some(button)),
        _ => (None, None),
    };
    for doc in editor.documents.values_mut() {
        doc.review_draft_cursor = cursor;
        doc.review_draft_button = button;
    }
}

/// Sync comments to every open document after review changes.
pub fn sync_all_open_documents(editor: &mut crate::Editor) {
    flush_pending_comment_remaps(editor);

    let pending = editor.review.pending_comment.clone();
    let Some(review) = editor.review.current.clone() else {
        for doc in editor.documents.values_mut() {
            doc.review_comments.clear();
        }
        return;
    };

    let repo_root = review.metadata.repo_root.clone();
    let doc_ids: Vec<_> = editor.documents.keys().copied().collect();
    for doc_id in doc_ids {
        if let Some(doc) = editor.documents.get_mut(&doc_id) {
            sync_comments_to_document(&review, doc);
            if let Some(pending) = &pending {
                apply_pending_comment_to_document(pending, doc, &repo_root);
            }
        }
    }
}

/// Normalize a comment file path for storage in a review session.
pub fn normalize_comment_path(path: &Path, repo_root: &Path) -> std::path::PathBuf {
    let canonical = helix_stdx::path::canonicalize(path);
    normalize_path(&canonical, repo_root)
}

pub use helix_review::{
    author_label, capture_line_context, comment_box_height, comment_header_text,
    comment_text_format, create_new_review, delete_review, format_comment_timestamp,
    format_review_for_llm, list_reviews, load_review, new_comment_id, parse_unified_diff_line_map,
    repo_slug, save_review, timestamp_now, CommentAuthor, DiffLineMapping, DiffReviewSource,
    DiffSide, ReviewComment, ReviewData, ReviewListEntry, ReviewMetadata, ReviewStatus,
};
