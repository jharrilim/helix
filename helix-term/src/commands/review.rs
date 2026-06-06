use std::path::PathBuf;

use helix_loader::find_workspace;
use helix_view::editor::Action;
use helix_view::{
    align_view, capture_line_context, create_new_review, diff_metadata_for_line,
    format_review_for_llm, list_reviews, load_review, new_comment_id, normalize_comment_path,
    review::{ReviewDraftButton, ReviewDraftFocus},
    delete_review, save_review, sync_all_open_documents, Align, Editor, PendingReviewComment,
    ReviewComment, ReviewPanelSelection,
    ReviewListEntry, ReviewStatus, timestamp_now,
};
use helix_view::input::Event;

use super::Context;
use crate::agent;
use crate::compositor::Compositor;
use crate::handlers::agent::send_prompt;
use crate::job::Jobs;
use crate::key;
use crate::ui::{overlay::overlaid, Prompt, PromptEvent, Picker, PickerColumn};

const CONTEXT_LINES: usize = 3;

/// Input for appending an agent-authored review reply comment.
#[derive(Debug, Clone)]
pub struct AgentReviewReplyInput {
    pub review_id: Option<String>,
    pub comment_id: Option<String>,
    pub file_path: Option<PathBuf>,
    pub line: Option<usize>,
    pub body: String,
}

/// Output metadata after writing an agent-authored review reply.
#[derive(Debug, Clone)]
pub struct AgentReviewReplyOutput {
    pub comment_id: String,
    pub review_id: String,
}

/// Append an agent-authored reply to a loaded review payload.
pub fn append_agent_review_reply_to_review(
    review: &mut helix_view::ReviewData,
    input: &AgentReviewReplyInput,
) -> Result<String, String> {
    let repo_root = review.metadata.repo_root.clone();
    let target = if let Some(parent_id) = input.comment_id.as_deref() {
        let parent = review
            .comments
            .iter()
            .find(|comment| comment.id == parent_id)
            .cloned()
            .ok_or_else(|| format!("review comment id '{parent_id}' not found"))?;

        if let (Some(file_path), Some(line)) = (input.file_path.as_ref(), input.line) {
            let normalized = normalize_comment_path(file_path, &repo_root);
            if normalized != parent.file || line != parent.line {
                return Err(
                    "comment_id target conflicts with explicit file_path/line target".into(),
                );
            }
        }

        (
            parent.file,
            parent.line,
            parent.context_before,
            parent.code_at_comment,
            parent.context_after,
            parent.diff_side,
            parent.hunk_index,
        )
    } else {
        let file_path = input
            .file_path
            .as_ref()
            .ok_or_else(|| "file_path is required when comment_id is not provided".to_string())?;
        let line = input
            .line
            .ok_or_else(|| "line is required when comment_id is not provided".to_string())?;
        (
            normalize_comment_path(file_path, &repo_root),
            line,
            Vec::new(),
            String::new(),
            Vec::new(),
            None,
            None,
        )
    };

    let body = input.body.trim().to_string();
    if let Some(existing) = review.comments.last().filter(|comment| {
        comment.author == helix_view::CommentAuthor::Agent
            && comment.file == target.0
            && comment.line == target.1
            && comment.body == body
    }) {
        return Ok(existing.id.clone());
    }
    let created_at = timestamp_now();

    let mut comment_id = new_comment_id();
    if review.comments.iter().any(|comment| comment.id == comment_id) {
        let mut suffix = 1usize;
        while review
            .comments
            .iter()
            .any(|comment| comment.id == format!("{comment_id}-{suffix}"))
        {
            suffix += 1;
        }
        comment_id = format!("{comment_id}-{suffix}");
    }

    let comment = ReviewComment {
        id: comment_id,
        file: target.0,
        line: target.1,
        line_end: None,
        char_idx: 0,
        body,
        author: helix_view::CommentAuthor::Agent,
        context_before: target.2,
        context_after: target.4,
        code_at_comment: target.3,
        diff_side: target.5,
        hunk_index: target.6,
        created_at,
    };
    let comment_id = comment.id.clone();
    review.comments.push(comment);
    Ok(comment_id)
}

pub fn review_toggle_editor(editor: &mut Editor) {
    editor.review.ensure_repo_slug();
    if editor.review.active {
        editor.review.active = false;
        clear_pending_review_comment(editor);
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
    if editor.review_panel.is_open() {
        crate::ui::review_panel::refresh_list(editor);
    }
}

pub fn review_comment_compositor(compositor: &mut Compositor, editor: &mut Editor) {
    push_review_comment_prompt(compositor, editor, false);
}

fn clear_pending_review_comment(editor: &mut Editor) {
    editor.review.pending_comment = None;
    editor.review.draft_focus = ReviewDraftFocus::Prompt;
    sync_all_open_documents(editor);
}

fn push_review_comment_prompt(compositor: &mut Compositor, editor: &mut Editor, reopen: bool) {
    let Some(mut prompt) = build_review_comment_prompt(editor, reopen) else {
        return;
    };
    if reopen {
        if let Some(pending) = editor.review.pending_comment.as_ref() {
            prompt.set_line(pending.body.clone(), editor);
        }
    }
    prompt.recalculate_completion(editor);
    compositor.need_full_redraw();
    compositor.push(Box::new(prompt));
}

/// Handle keys while draft footer buttons are focused.
pub fn handle_review_draft_button_event(cx: &mut Context, event: &Event) -> bool {
    let ReviewDraftFocus::Buttons(button) = cx.editor.review.draft_focus else {
        return false;
    };
    if cx.editor.review.pending_comment.is_none() {
        return false;
    }

    let Event::Key(key) = event else {
        return true;
    };

    match key {
        key!(Esc) => {
            cancel_pending_review_comment_cmd(cx);
            true
        }
        key!(Enter) => {
            match button {
                ReviewDraftButton::Save => commit_pending_review_comment(cx),
                ReviewDraftButton::Delete => cancel_pending_review_comment_cmd(cx),
            }
            true
        }
        key!(Left) => {
            cx.editor.review.draft_focus =
                ReviewDraftFocus::Buttons(ReviewDraftButton::Delete);
            cx.editor.set_status(review_button_status());
            helix_event::request_redraw();
            true
        }
        key!(Right) => {
            cx.editor.review.draft_focus = ReviewDraftFocus::Buttons(ReviewDraftButton::Save);
            cx.editor.set_status(review_button_status());
            helix_event::request_redraw();
            true
        }
        key!(Up) => {
            cx.editor.review.draft_focus = ReviewDraftFocus::Prompt;
            cx.editor
                .set_status("Enter review comment (Esc to cancel, Enter for actions)");
            cx.callback.push(Box::new(|compositor, cx| {
                push_review_comment_prompt(compositor, cx.editor, true);
            }));
            helix_event::request_redraw();
            true
        }
        _ => true,
    }
}

fn review_button_status() -> String {
    "←/→ choose Save or Delete, Enter to confirm, ↑ to edit, Esc to cancel".into()
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

fn cancel_pending_review_comment(cx: &mut crate::compositor::Context) {
    clear_pending_review_comment(cx.editor);
    cx.editor.set_status("review comment cancelled");
    helix_event::request_redraw();
}

fn cancel_pending_review_comment_cmd(cx: &mut Context) {
    clear_pending_review_comment(cx.editor);
    cx.editor.set_status("review comment cancelled");
    helix_event::request_redraw();
}

fn commit_pending_review_comment(cx: &mut Context) {
    let Some(body) = cx
        .editor
        .review
        .pending_comment
        .as_ref()
        .map(|pending| pending.body.trim().to_string())
    else {
        return;
    };
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
    let Some(raw_path) = resolve_comment_file_path(doc.path(), diff_source.as_ref(), line) else {
        cx.editor
            .set_error("cannot comment: buffer has no associated file path");
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
        author: helix_view::CommentAuthor::User,
        context_before,
        context_after,
        code_at_comment,
        diff_side,
        hunk_index,
        created_at: timestamp_now(),
    };
    review.comments.push(comment);

    if let Err(err) = save_review(&repo_slug, review) {
        cx.editor
            .set_error(format!("failed to save review: {err:#}"));
        return;
    }

    clear_pending_review_comment(cx.editor);
    cx.editor.set_status("review comment added");
    helix_event::request_redraw();
}

fn build_review_comment_prompt(editor: &mut Editor, reopen: bool) -> Option<Prompt> {
    if !editor.review.active {
        editor
            .set_error("review mode is not active (use :review-toggle)");
        return None;
    }

    if reopen && editor.review.pending_comment.is_none() {
        return None;
    }

    if !reopen {
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
        editor.review.draft_focus = ReviewDraftFocus::Prompt;
        sync_all_open_documents(editor);
        editor.set_status("Enter review comment (Esc to cancel, Enter for actions)");
    } else if editor.review.current.is_none() {
        editor.set_error("no active review session");
        return None;
    }

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
                    cancel_pending_review_comment(cx);
                }
                PromptEvent::Advance => {
                    if let Some(pending) = cx.editor.review.pending_comment.as_mut() {
                        pending.body = body.to_string();
                    }
                    sync_all_open_documents(cx.editor);
                    cx.editor.review.draft_focus =
                        ReviewDraftFocus::Buttons(ReviewDraftButton::Save);
                    cx.editor.set_status(review_button_status());
                    helix_event::request_redraw();
                }
                PromptEvent::Validate => {}
            },
        )
        .multiline()
        .retain_editor()
        .submit_to_footer_buttons(),
    )
}

pub fn review_comment_editor(cx: &mut Context) {
    cx.callback.push(Box::new(|compositor, cx| {
        push_review_comment_prompt(compositor, cx.editor, false);
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

pub fn resume_review(editor: &mut Editor, repo_slug: &str, review_id: &str) {
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

pub fn open_comment_location_editor(editor: &mut Editor, comment: &ReviewComment) {
    if editor.tree.is_review_panel(editor.tree.focus) {
        editor.focus_editor_from_review_panel();
    } else if !editor.is_document_view_focused() {
        editor.focus_editor_from_review_panel();
    }

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

/// Append an `Agent` review reply comment and persist/sync editor state.
pub fn add_agent_review_reply_editor(
    editor: &mut Editor,
    input: AgentReviewReplyInput,
) -> Result<AgentReviewReplyOutput, String> {
    let body = input.body.trim().to_string();
    if body.is_empty() {
        return Err("review reply body is empty".into());
    }

    editor.review.ensure_repo_slug();
    let repo_slug = editor.review.repo_slug.clone();

    let (review_id, comment_id) = {
        let review = editor
            .review
            .current
            .as_mut()
            .ok_or_else(|| "no active review session".to_string())?;

        if let Some(expected_review_id) = input.review_id.as_deref() {
            if review.metadata.id != expected_review_id {
                return Err(format!(
                    "active review id '{}' does not match requested '{}'",
                    review.metadata.id, expected_review_id
                ));
            }
        }

        let comment_id = append_agent_review_reply_to_review(review, &input)?;
        let review_id = review.metadata.id.clone();
        save_review(&repo_slug, review).map_err(|err| format!("failed to save review: {err:#}"))?;
        (review_id, comment_id)
    };

    sync_all_open_documents(editor);
    if editor.review_panel.is_open() {
        crate::ui::review_panel::refresh_list(editor);
    }
    editor.set_status("agent review reply added");
    helix_event::request_redraw();

    Ok(AgentReviewReplyOutput {
        comment_id,
        review_id,
    })
}

pub fn review_panel_toggle(cx: &mut Context) {
    if cx.editor.review_panel.is_open() {
        if cx.editor.tree.is_review_panel(cx.editor.tree.focus) {
            cx.editor.close_review_panel();
        } else {
            cx.editor.focus_review_panel();
            crate::ui::review_panel::refresh_list(cx.editor);
        }
    } else {
        cx.editor.open_review_panel();
        cx.editor.review.active = true;
        crate::ui::review_panel::refresh_list(cx.editor);
    }
    helix_event::request_redraw();
}

pub fn review_panel_activate_editor(editor: &mut Editor) {
    let Some(selection) = editor.review_panel.selection else {
        return;
    };
    editor.review.ensure_repo_slug();
    let repo_slug = editor.review.repo_slug.clone();
    match selection {
        ReviewPanelSelection::Review(index) => {
            let Some(entry) = editor.review_panel.entries.get(index) else {
                return;
            };
            let id = entry.id.clone();
            resume_review(editor, &repo_slug, &id);
            editor.review.navigation_index = 0;
            crate::ui::review_panel::refresh_list(editor);
        }
        ReviewPanelSelection::Comment(index) => {
            let Some(review) = editor.review.current.clone() else {
                editor.set_error("no active review session");
                return;
            };
            if index >= review.comments.len() {
                return;
            }
            editor.review.navigation_index = index;
            let comment = review.comments[index].clone();
            open_comment_location_editor(editor, &comment);
        }
    }
    helix_event::request_redraw();
}

pub fn review_panel_delete_editor(editor: &mut Editor) {
    let Some(selection) = editor.review_panel.selection else {
        editor.set_error("nothing selected in review panel");
        return;
    };
    editor.review.ensure_repo_slug();
    let repo_slug = editor.review.repo_slug.clone();

    match selection {
        ReviewPanelSelection::Review(index) => {
            let Some(entry) = editor.review_panel.entries.get(index) else {
                return;
            };
            let id = entry.id.clone();
            if let Err(err) = delete_review(&repo_slug, &id) {
                editor.set_error(format!("failed to delete review: {err:#}"));
                return;
            }
            if editor
                .review
                .current
                .as_ref()
                .is_some_and(|r| r.metadata.id == id)
            {
                editor.review.current = None;
                editor.review.active = false;
                sync_all_open_documents(editor);
            }
            crate::ui::review_panel::refresh_list(editor);
            editor.set_status(format!("deleted review {id}"));
        }
        ReviewPanelSelection::Comment(index) => {
            let removed = editor.review.current.as_mut().and_then(|review| {
                if index >= review.comments.len() {
                    return None;
                }
                review.comments.remove(index);
                Some(())
            });
            if removed.is_none() {
                editor.set_error("no active review session");
                return;
            }
            let Some(review) = editor.review.current.as_ref() else {
                return;
            };
            if let Err(err) = save_review(&repo_slug, review) {
                editor.set_error(format!("failed to save review: {err:#}"));
                return;
            }
            sync_all_open_documents(editor);
            let comment_count = editor.review.current.as_ref().map(|r| r.comments.len()).unwrap_or(0);
            if comment_count > 0 {
                editor.review.navigation_index = editor.review.navigation_index.min(comment_count - 1);
                editor.review_panel.selection =
                    Some(ReviewPanelSelection::Comment(editor.review.navigation_index));
            } else {
                editor.review_panel.selection = editor
                    .review_panel
                    .entries
                    .iter()
                    .position(|entry| {
                        editor
                            .review
                            .current
                            .as_ref()
                            .is_some_and(|r| r.metadata.id == entry.id)
                    })
                    .map(ReviewPanelSelection::Review);
            }
            editor.set_status("review comment deleted");
        }
    }
    helix_event::request_redraw();
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
