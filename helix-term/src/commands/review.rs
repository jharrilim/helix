use std::path::PathBuf;

use helix_loader::find_workspace;
use helix_view::editor::Action;
use helix_view::{
    align_view, capture_line_context, create_new_review, diff_metadata_for_line,
    format_review_for_llm, list_reviews, load_review, new_comment_id, normalize_comment_path,
    save_review, sync_all_open_documents, Align, Editor, PendingReviewComment, ReviewComment,
    ReviewListEntry, ReviewStatus, timestamp_now,
};

use super::Context;
use crate::agent;
use crate::compositor::Compositor;
use crate::handlers::agent::send_prompt;
use crate::job::Jobs;
use crate::ui::{overlay::overlaid, Prompt, PromptEvent, Picker, PickerColumn};

const CONTEXT_LINES: usize = 3;

pub fn review_toggle_editor(editor: &mut Editor) {
    editor.review.ensure_repo_slug();
    if editor.review.active {
        editor.review.active = false;
        editor.review.pending_comment = None;
        sync_all_open_documents(editor);
        editor.set_status("review mode disabled");
        return;
    }

    if editor.review.current.is_none() {
        let (repo_root, _) = find_workspace();
        let repo_slug = editor.review.repo_slug.clone();
        editor.review.current = Some(create_new_review(&repo_root, &repo_slug, "Untitled review"));
    }

    editor.review.active = true;
    sync_all_open_documents(editor);
    editor.set_status("review mode enabled");
}

pub fn review_new_editor(editor: &mut Editor) {
    editor.review.ensure_repo_slug();
    let (repo_root, _) = find_workspace();
    let repo_slug = editor.review.repo_slug.clone();
    editor.review.current = Some(create_new_review(&repo_root, &repo_slug, "Untitled review"));
    editor.review.active = true;
    editor.review.navigation_index = 0;
    sync_all_open_documents(editor);
    editor.set_status("started new review");
}

pub fn review_comment_compositor(compositor: &mut Compositor, editor: &mut Editor) {
    let Some(mut prompt) = build_review_comment_prompt(editor) else {
        return;
    };
    prompt.recalculate_completion(editor);
    compositor.need_full_redraw();
    compositor.push(Box::new(prompt));
}

struct CommentAnchor {
    raw_path: PathBuf,
    display_line: usize,
    source_line: usize,
}

fn capture_comment_anchor(editor: &mut Editor) -> Option<CommentAnchor> {
    let Some((view, doc)) = helix_view::try_current!(editor) else {
        editor.set_error("no document focused");
        return None;
    };
    let line = doc
        .selection(view.id)
        .primary()
        .cursor_line(doc.text().slice(..));
    let diff_source = doc.diff_review_source.as_ref();
    let Some(raw_path) = resolve_comment_file_path(doc.path(), diff_source, line) else {
        editor.set_error("cannot comment: buffer has no associated file path");
        return None;
    };
    let source_line = if let Some(diff_source) = diff_source {
        diff_source
            .line_map
            .get(line)
            .and_then(|mapping| mapping.as_ref())
            .map(|mapping| mapping.source_line)
            .unwrap_or(line)
    } else {
        line
    };
    Some(CommentAnchor {
        raw_path,
        display_line: line,
        source_line,
    })
}

fn build_review_comment_prompt(editor: &mut Editor) -> Option<Prompt> {
    if !editor.review.active {
        editor
            .set_error("review mode is not active (use :review-toggle)");
        return None;
    }

    let anchor = capture_comment_anchor(editor)?;
    let Some(review) = editor.review.current.as_ref() else {
        editor.set_error("no active review session");
        return None;
    };
    let repo_root = review.metadata.repo_root.clone();
    let file_path = normalize_comment_path(&anchor.raw_path, &repo_root);

    editor.review.pending_comment = Some(PendingReviewComment {
        file: file_path,
        display_line: anchor.display_line,
        source_line: anchor.source_line,
        body: String::new(),
    });
    sync_all_open_documents(editor);
    editor.set_status("Enter review comment (Esc to cancel)");

    Some(
        Prompt::new(
            "Review comment: ".into(),
            None,
            crate::ui::completers::none,
            |cx, body, event| match event {
            PromptEvent::Update => {
                if let Some(pending) = cx.editor.review.pending_comment.as_mut() {
                    pending.body = body.to_string();
                    sync_all_open_documents(cx.editor);
                    helix_event::request_redraw();
                }
            }
            PromptEvent::Abort => {
                cx.editor.review.pending_comment = None;
                sync_all_open_documents(cx.editor);
            }
            PromptEvent::Validate => {
                let body = body.trim().to_string();
                if body.is_empty() {
                    cx.editor.set_error("review comment is empty");
                    return;
                }

                if !cx.editor.review.active {
                    cx.editor
                        .set_error("review mode is not active (use :review-toggle)");
                    return;
                }

                let Some((view, doc)) = helix_view::try_current!(cx.editor) else {
                    cx.editor.set_error("no document focused");
                    return;
                };

                let line = doc
                    .selection(view.id)
                    .primary()
                    .cursor_line(doc.text().slice(..));
                let diff_source = doc.diff_review_source.clone();
                let Some(raw_path) =
                    resolve_comment_file_path(doc.path(), diff_source.as_ref(), line)
                else {
                    cx.editor.set_error("cannot comment: buffer has no associated file path");
                    return;
                };

                let source_line = if let Some(diff_source) = &diff_source {
                    diff_source
                        .line_map
                        .get(line)
                        .and_then(|mapping| mapping.as_ref())
                        .map(|mapping| mapping.source_line)
                        .unwrap_or(line)
                } else {
                    line
                };

                let text = doc.text().slice(..);
                let (context_before, code_at_comment, context_after) =
                    capture_line_context(text, line, CONTEXT_LINES);

                let (hunk_index, diff_side) = if diff_source.is_some() {
                    diff_source
                        .as_ref()
                        .and_then(|source| source.line_map.get(line))
                        .and_then(|mapping| mapping.as_ref())
                        .map(|mapping| (None, Some(mapping.side)))
                        .unwrap_or((None, None))
                } else {
                    diff_metadata_for_line(doc, line as u32)
                };

                let char_idx = doc.text().line_to_char(line);

                cx.editor.review.ensure_repo_slug();
                let repo_slug = cx.editor.review.repo_slug.clone();
                let Some(review) = cx.editor.review.current.as_mut() else {
                    cx.editor.set_error("no active review session");
                    return;
                };

                let repo_root = review.metadata.repo_root.clone();
                let file_path = normalize_comment_path(&raw_path, &repo_root);

                let comment = ReviewComment {
                    id: new_comment_id(),
                    file: file_path,
                    line: source_line,
                    line_end: None,
                    char_idx,
                    body,
                    context_before,
                    context_after,
                    code_at_comment,
                    diff_side,
                    hunk_index,
                    created_at: timestamp_now(),
                };
                review.comments.push(comment);

                if let Err(err) = save_review(&repo_slug, review) {
                    cx.editor.set_error(format!("failed to save review: {err:#}"));
                    return;
                }

                cx.editor.review.pending_comment = None;
                sync_all_open_documents(cx.editor);
                cx.editor.set_status("review comment added");
            }
        },
        )
        .retain_editor(),
    )
}

pub fn review_comment_editor(cx: &mut Context) {
    let Some(mut prompt) = build_review_comment_prompt(cx.editor) else {
        return;
    };
    prompt.recalculate_completion(cx.editor);
    cx.callback.push(Box::new(|compositor, _| {
        compositor.need_full_redraw();
        compositor.push(Box::new(prompt));
    }));
}

fn resolve_comment_file_path(
    doc_path: Option<&PathBuf>,
    diff_source: Option<&helix_view::DiffReviewSource>,
    line: usize,
) -> Option<PathBuf> {
    if let Some(source) = diff_source {
        return source
            .line_map
            .get(line)
            .and_then(|mapping| mapping.as_ref())
            .map(|mapping| mapping.source_path.clone())
            .or_else(|| Some(source.source_path.clone()));
    }
    doc_path.map(|path| path.to_path_buf())
}

fn build_review_summary_prompt() -> Prompt {
    Prompt::new(
        "Review summary: ".into(),
        None,
        crate::ui::completers::none,
        |cx, summary, event| {
            if event != PromptEvent::Validate {
                return;
            }
            cx.editor.review.ensure_repo_slug();
            let repo_slug = cx.editor.review.repo_slug.clone();
            let Some(review) = cx.editor.review.current.as_mut() else {
                return;
            };
            review.metadata.summary = summary.trim().to_string();
            if let Err(err) = save_review(&repo_slug, review) {
                cx.editor.set_error(format!("failed to save review: {err:#}"));
                return;
            }
            cx.editor.set_status("review summary saved");
        },
    )
}

pub fn review_summary_compositor(compositor: &mut Compositor, editor: &mut Editor) {
    if editor.review.current.is_none() {
        editor.set_error("no active review session");
        return;
    }

    compositor.push(Box::new(build_review_summary_prompt()));
}

pub fn review_list_compositor(compositor: &mut Compositor, editor: &mut Editor) {
    editor.review.ensure_repo_slug();
    let repo_slug = editor.review.repo_slug.clone();
    let entries = list_reviews(&repo_slug);
    if entries.is_empty() {
        editor.set_error("no saved reviews for this repository");
        return;
    }

    let columns = [
        PickerColumn::new("id", |entry: &ReviewListEntry, _| entry.id.as_str().into()),
        PickerColumn::new("title", |entry: &ReviewListEntry, _| entry.title.as_str().into()),
        PickerColumn::new("comments", |entry: &ReviewListEntry, _| {
            entry.comment_count.to_string().into()
        }),
    ];

    let picker = Picker::new(columns, 0, entries, (), move |cx, entry, _action| {
        resume_review(cx.editor, &repo_slug, &entry.id);
    });
    compositor.push(Box::new(overlaid(picker)));
}

pub fn review_submit_editor(editor: &mut Editor, jobs: &mut Jobs) {
    let Some(review) = editor.review.current.clone() else {
        editor.set_error("no active review session");
        return;
    };

    if review.comments.is_empty() && review.metadata.summary.is_empty() {
        editor.set_error("review has no comments or summary");
        return;
    }

    let prompt = format_review_for_llm(&review);
    crate::commands::agent::agent_open_editor(editor, jobs);

    if let Err(err) = agent::ensure_runtime(editor, jobs) {
        editor.set_error(format!("{err:#}"));
        return;
    }

    agent::with_controller(|controller| {
        send_prompt(controller, editor, prompt);
    });

    if let Some(current) = editor.review.current.as_mut() {
        current.metadata.status = ReviewStatus::Submitted;
        current.metadata.submitted_at = Some(timestamp_now());
        let repo_slug = editor.review.repo_slug.clone();
        if let Err(err) = save_review(&repo_slug, current) {
            editor.set_error(format!("review submitted but failed to save: {err:#}"));
        }
    }

    editor.set_status("review submitted to agent");
}

pub fn review_summary_editor(cx: &mut Context) {
    if cx.editor.review.current.is_none() {
        cx.editor.set_error("no active review session");
        return;
    }

    cx.push_layer(Box::new(build_review_summary_prompt()));
}

pub fn review_list_editor(cx: &mut Context) {
    cx.editor.review.ensure_repo_slug();
    let repo_slug = cx.editor.review.repo_slug.clone();
    let entries = list_reviews(&repo_slug);
    if entries.is_empty() {
        cx.editor.set_error("no saved reviews for this repository");
        return;
    }

    let columns = [
        PickerColumn::new("id", |entry: &ReviewListEntry, _| entry.id.as_str().into()),
        PickerColumn::new("title", |entry: &ReviewListEntry, _| entry.title.as_str().into()),
        PickerColumn::new("comments", |entry: &ReviewListEntry, _| {
            entry.comment_count.to_string().into()
        }),
    ];

    let picker = Picker::new(columns, 0, entries, (), move |cx, entry, _action| {
        resume_review(cx.editor, &repo_slug, &entry.id);
    });
    cx.push_layer(Box::new(overlaid(picker)));
}

pub fn review_resume_editor(editor: &mut Editor, review_id: &str) {
    editor.review.ensure_repo_slug();
    let repo_slug = editor.review.repo_slug.clone();
    resume_review(editor, &repo_slug, review_id);
}

fn resume_review(editor: &mut Editor, repo_slug: &str, review_id: &str) {
    match load_review(repo_slug, review_id) {
        Ok(review) => {
            editor.review.current = Some(review);
            editor.review.active = true;
            editor.review.navigation_index = 0;
            sync_all_open_documents(editor);
            editor.set_status(format!("resumed review {review_id}"));
        }
        Err(err) => editor.set_error(format!("failed to load review: {err:#}")),
    }
}

pub fn review_next_editor(editor: &mut Editor) {
    navigate_review_comment_editor(editor, true);
}

pub fn review_prev_editor(editor: &mut Editor) {
    navigate_review_comment_editor(editor, false);
}

fn navigate_review_comment_editor(editor: &mut Editor, forward: bool) {
    let Some(review) = editor.review.current.clone() else {
        editor.set_error("no active review session");
        return;
    };

    if review.comments.is_empty() {
        editor.set_error("review has no comments");
        return;
    }

    let len = review.comments.len();
    let index = if forward {
        (editor.review.navigation_index + 1) % len
    } else {
        editor
            .review
            .navigation_index
            .checked_sub(1)
            .unwrap_or(len - 1)
    };
    editor.review.navigation_index = index;

    let comment = review.comments[index].clone();
    open_comment_location_editor(editor, &comment);
}

fn open_comment_location_editor(editor: &mut Editor, comment: &ReviewComment) {
    let path = comment_open_path(editor, comment);
    let line = comment.line;
    if let Err(err) = editor.open(&path, Action::Replace) {
        editor
            .set_error(format!("failed to open {}: {err}", path.display()));
        return;
    }

    sync_all_open_documents(editor);

    let Some((view, doc)) = helix_view::try_current!(editor) else {
        return;
    };
    let text = doc.text().slice(..);
    let char_idx = doc.text().line_to_char(line.min(text.len_lines().saturating_sub(1)));
    doc.set_selection(view.id, helix_core::Selection::point(char_idx));
    align_view(doc, view, Align::Center);
    let total = editor
        .review
        .current
        .as_ref()
        .map(|review| review.comments.len())
        .unwrap_or(0);
    editor.set_status(format!(
        "review comment {}/{}",
        editor.review.navigation_index + 1,
        total
    ));
}

fn comment_open_path(editor: &Editor, comment: &ReviewComment) -> PathBuf {
    if comment.file.is_absolute() {
        return comment.file.clone();
    }

    editor
        .review
        .current
        .as_ref()
        .map(|review| review.metadata.repo_root.join(&comment.file))
        .unwrap_or_else(|| comment.file.clone())
}

pub fn review_toggle(cx: &mut Context) {
    review_toggle_editor(cx.editor);
}

pub fn review_comment(cx: &mut Context) {
    review_comment_editor(cx);
}

pub fn review_submit(cx: &mut Context) {
    review_submit_editor(cx.editor, cx.jobs);
}

pub fn review_new(cx: &mut Context) {
    review_new_editor(cx.editor);
}

pub fn review_list(cx: &mut Context) {
    review_list_editor(cx);
}

pub fn review_summary(cx: &mut Context) {
    review_summary_editor(cx);
}

pub fn review_next(cx: &mut Context) {
    review_next_editor(cx.editor);
}

pub fn review_prev(cx: &mut Context) {
    review_prev_editor(cx.editor);
}
