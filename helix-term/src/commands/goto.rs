use super::prelude::*;
use super::{Context, helpers::*};

pub fn goto_file(cx: &mut Context) {
    goto_file_impl(cx, Action::Replace);
}

pub fn goto_file_hsplit(cx: &mut Context) {
    goto_file_impl(cx, Action::HorizontalSplit);
}

pub fn goto_file_vsplit(cx: &mut Context) {
    goto_file_impl(cx, Action::VerticalSplit);
}

/// Returns true when a selection overlaps an LSP document link range.
pub fn selection_overlaps_document_link(
    selection: &Range,
    link: &helix_view::document::DocumentLink,
) -> bool {
    if selection.is_empty() {
        let pos = selection.from();
        link.start <= pos && pos < link.end
    } else {
        selection.from() < link.end && selection.to() > link.start
    }
}

/// Create a document link resolve request when the target isn't already present.
///
/// This only builds the LSP request. The request is awaited from a background
/// job so `goto_file_impl` does not block the UI thread while the language
/// server resolves the target.
pub fn resolve_document_link_request(
    editor: &Editor,
    link: &helix_view::document::DocumentLink,
) -> Option<impl Future<Output = helix_lsp::Result<helix_lsp::lsp::DocumentLink>> + Send + 'static>
{
    let language_server = editor.language_server_by_id(link.language_server_id)?;
    let supports_resolve = language_server
        .capabilities()
        .document_link_provider
        .as_ref()?
        .resolve_provider
        .unwrap_or(false);

    if !supports_resolve {
        return None;
    }

    language_server.resolve_document_link(link.link.clone())
}

/// Goto files/URLs in selection.
///
/// Prefers LSP document links when the cursor/selection overlaps a link range,
/// falling back to the built-in path/URL detection otherwise.
pub fn goto_file_impl(cx: &mut Context, action: Action) {
    let (view, doc) = current_ref!(cx.editor);
    let text = doc.text().clone();
    let selections = doc.selection(view.id).ranges().to_vec();
    let rel_path = doc
        .relative_path()
        .map(|path| path.parent().unwrap().to_path_buf())
        .unwrap_or_default();
    let text = text.slice(..);

    let mut lsp_targets = Vec::new();
    let mut lsp_targets_seen = HashSet::new();
    let mut unresolved_links = HashSet::new();
    let mut resolve_requests = Vec::new();
    let mut fallback_ranges = Vec::new();

    if doc.document_links.is_empty() {
        fallback_ranges.extend_from_slice(&selections);
    } else {
        for selection in &selections {
            let mut matched = false;
            for link in &doc.document_links {
                if !selection_overlaps_document_link(selection, link) {
                    continue;
                }
                matched = true;
                if let Some(target) = link.link.target.clone() {
                    if lsp_targets_seen.insert(target.clone()) {
                        lsp_targets.push(target);
                    }
                } else if unresolved_links.insert((link.start, link.end, link.language_server_id)) {
                    if let Some(request) = resolve_document_link_request(cx.editor, link) {
                        resolve_requests.push(request);
                    }
                }
            }
            if !matched {
                fallback_ranges.push(*selection);
            }
        }
    }

    for target in lsp_targets {
        open_url(cx, target, action);
    }

    if !resolve_requests.is_empty() {
        let rel_path = rel_path.clone();
        cx.jobs.callback(async move {
            let mut targets = Vec::new();
            let mut seen = HashSet::new();

            // Resolve links off the main thread, then hand the resulting URLs
            // back to the editor/compositor callback once all requests finish.
            for request in resolve_requests {
                match request.await {
                    Ok(link) => {
                        if let Some(target) = link.target {
                            if seen.insert(target.clone()) {
                                targets.push(target);
                            }
                        }
                    }
                    Err(err) => log::warn!("Failed to resolve document link: {err}"),
                }
            }

            Ok(Callback::EditorCompositor(Box::new(
                move |editor, compositor| {
                    for target in targets {
                        open_url_in_callback(editor, compositor, target, action, &rel_path);
                    }
                },
            )))
        });
    }

    if fallback_ranges.is_empty() {
        return;
    }

    let paths: Vec<_> = if fallback_ranges.len() == 1 && fallback_ranges[0].len() == 1 {
        let selection = fallback_ranges[0];
        // Cap the search at roughly 1k bytes around the cursor.
        let lookaround = 1000;
        let pos = text.char_to_byte(selection.cursor(text));
        let search_start = text
            .line_to_byte(text.byte_to_line(pos))
            .max(text.floor_char_boundary(pos.saturating_sub(lookaround)));
        let search_end = text
            .line_to_byte(text.byte_to_line(pos) + 1)
            .min(text.ceil_char_boundary(pos + lookaround));
        let search_range = text.byte_slice(search_start..search_end);
        // we also allow paths that are next to the cursor (can be ambiguous but
        // rarely so in practice) so that gf on quoted/braced path works (not sure about this
        // but apparently that is how gf has worked historically in helix)
        let path = find_paths(search_range, true)
            .take_while(|range| search_start + range.start <= pos + 1)
            .find(|range| pos <= search_start + range.end)
            .map(|range| Cow::from(search_range.byte_slice(range)));
        log::debug!("goto_file auto-detected path: {path:?}");
        let path = path.unwrap_or_else(|| selection.fragment(text));
        vec![path.into_owned()]
    } else {
        // Otherwise use each selection, trimmed.
        fallback_ranges
            .iter()
            .map(|range| range.fragment(text).trim().to_owned())
            .filter(|sel| !sel.is_empty())
            .collect()
    };

    for sel in paths {
        if let Ok(url) = Url::parse(&sel) {
            open_url(cx, url, action);
            continue;
        }

        let path = path::expand(&sel);
        let path = &rel_path.join(path);
        if path.is_dir() {
            let picker = ui::file_picker(cx.editor, path.into());
            cx.push_layer(Box::new(overlaid(picker)));
        } else if let Err(e) = cx.editor.open(path, action) {
            cx.editor.set_error(format!("Open file failed: {:?}", e));
        }
    }
}
#[derive(PartialEq)]
pub enum Open {
    Below,
    Above,
}

#[derive(PartialEq)]
pub enum CommentContinuation {
    Enabled,
    Disabled,
}

pub fn open(cx: &mut Context, open_at: Open, comment_continuation: CommentContinuation) {
    let count = cx.count();
    enter_insert_mode(cx);
    let config = cx.editor.config();
    let (view, doc) = current!(cx.editor);
    let loader = cx.editor.syn_loader.load();

    let text = doc.text().slice(..);
    let contents = doc.text();
    let selection = doc.selection(view.id);
    let mut offs = 0;

    let mut ranges = SmallVec::with_capacity(selection.len());

    let continue_comment_tokens =
        if comment_continuation == CommentContinuation::Enabled && config.continue_comments {
            doc.language_config()
                .and_then(|config| config.comment_tokens.as_ref())
        } else {
            None
        };

    let mut transaction = Transaction::change_by_selection(contents, selection, |range| {
        // the line number, where the cursor is currently
        let curr_line_num = text.char_to_line(match open_at {
            Open::Below => graphemes::prev_grapheme_boundary(text, range.to()),
            Open::Above => range.from(),
        });

        // the next line number, where the cursor will be, after finishing the transaction
        let next_new_line_num = match open_at {
            Open::Below => curr_line_num + 1,
            Open::Above => curr_line_num,
        };

        let above_next_new_line_num = next_new_line_num.saturating_sub(1);

        let continue_comment_token = continue_comment_tokens
            .and_then(|tokens| comment::get_comment_token(text, tokens, curr_line_num));

        // Index to insert newlines after, as well as the char width
        // to use to compensate for those inserted newlines.
        let (above_next_line_end_index, above_next_line_end_width) = if next_new_line_num == 0 {
            (0, 0)
        } else {
            (
                line_end_char_index(&text, above_next_new_line_num),
                doc.line_ending.len_chars(),
            )
        };

        let line = text.line(curr_line_num);
        let indent = match line.first_non_whitespace_char() {
            Some(pos) if continue_comment_token.is_some() => line.slice(..pos).to_string(),
            _ => indent::indent_for_newline(
                &loader,
                doc.syntax(),
                &config.indent_heuristic,
                &doc.indent_style,
                doc.tab_width(),
                text,
                above_next_new_line_num,
                above_next_line_end_index,
                curr_line_num,
            ),
        };

        let indent_len = indent.len();
        let mut text = String::with_capacity(1 + indent_len);

        if open_at == Open::Above && next_new_line_num == 0 {
            text.push_str(&indent);
            if let Some(token) = continue_comment_token {
                text.push_str(token);
                text.push(' ');
            }
            text.push_str(doc.line_ending.as_str());
        } else {
            text.push_str(doc.line_ending.as_str());
            text.push_str(&indent);

            if let Some(token) = continue_comment_token {
                text.push_str(token);
                text.push(' ');
            }
        }

        let text = text.repeat(count);

        // calculate new selection ranges
        let pos = offs + above_next_line_end_index + above_next_line_end_width;
        let comment_len = continue_comment_token
            .map(|token| token.len() + 1) // `+ 1` for the extra space added
            .unwrap_or_default();
        for i in 0..count {
            // pos                     -> beginning of reference line,
            // + (i * (line_ending_len + indent_len + comment_len)) -> beginning of i'th line from pos (possibly including comment token)
            // + indent_len + comment_len ->        -> indent for i'th line
            ranges.push(Range::point(
                pos + (i * (doc.line_ending.len_chars() + indent_len + comment_len))
                    + indent_len
                    + comment_len,
            ));
        }

        // update the offset for the next range
        offs += text.chars().count();

        (
            above_next_line_end_index,
            above_next_line_end_index,
            Some(text.into()),
        )
    });

    transaction = transaction.with_selection(Selection::new(ranges, selection.primary_index()));

    doc.apply(&transaction, view.id);
}

// o inserts a new line after each line with a selection
pub fn open_below(cx: &mut Context) {
    open(cx, Open::Below, CommentContinuation::Enabled)
}

// O inserts a new line before each line with a selection
pub fn open_above(cx: &mut Context) {
    open(cx, Open::Above, CommentContinuation::Enabled)
}
pub fn goto_line(cx: &mut Context) {
    goto_line_impl(cx, Movement::Move);
}

pub fn goto_line_impl(cx: &mut Context, movement: Movement) {
    if cx.count.is_some() {
        let (view, doc) = current!(cx.editor);
        push_jump(view, doc);

        goto_line_without_jumplist(cx.editor, cx.count, movement);
    }
}

pub fn goto_last_line(cx: &mut Context) {
    goto_last_line_impl(cx, Movement::Move)
}

pub fn extend_to_last_line(cx: &mut Context) {
    goto_last_line_impl(cx, Movement::Extend)
}

pub fn goto_last_line_impl(cx: &mut Context, movement: Movement) {
    let (view, doc) = current!(cx.editor);
    let text = doc.text().slice(..);
    let line_idx = if text.line(text.len_lines() - 1).len_chars() == 0 {
        // If the last line is blank, don't jump to it.
        text.len_lines().saturating_sub(2)
    } else {
        text.len_lines() - 1
    };
    let pos = text.line_to_char(line_idx);
    let selection = doc
        .selection(view.id)
        .clone()
        .transform(|range| range.put_cursor(text, pos, movement == Movement::Extend));

    push_jump(view, doc);
    doc.set_selection(view.id, selection);
}

pub fn goto_column(cx: &mut Context) {
    goto_column_impl(cx, Movement::Move);
}

pub fn extend_to_column(cx: &mut Context) {
    goto_column_impl(cx, Movement::Extend);
}

pub fn goto_column_impl(cx: &mut Context, movement: Movement) {
    let count = cx.count();
    let (view, doc) = current!(cx.editor);
    let text = doc.text().slice(..);
    let selection = doc.selection(view.id).clone().transform(|range| {
        let line = range.cursor_line(text);
        let line_start = text.line_to_char(line);
        let line_end = line_end_char_index(&text, line);
        let pos = graphemes::nth_next_grapheme_boundary(text, line_start, count - 1).min(line_end);
        range.put_cursor(text, pos, movement == Movement::Extend)
    });
    push_jump(view, doc);
    doc.set_selection(view.id, selection);
}

pub fn goto_last_accessed_file(cx: &mut Context) {
    let view = view_mut!(cx.editor);
    if let Some(alt) = view.docs_access_history.pop() {
        cx.editor.switch(alt, Action::Replace);
    } else {
        cx.editor.set_error("no last accessed buffer")
    }
}

pub fn goto_last_modification(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);
    let pos = doc.history.get_mut().last_edit_pos();
    let text = doc.text().slice(..);
    if let Some(pos) = pos {
        let selection = doc
            .selection(view.id)
            .clone()
            .transform(|range| range.put_cursor(text, pos, cx.editor.mode == Mode::Select));
        push_jump(view, doc);
        doc.set_selection(view.id, selection);
    }
}

pub fn goto_last_modified_file(cx: &mut Context) {
    let view = view!(cx.editor);
    let alternate_file = view
        .last_modified_docs
        .into_iter()
        .flatten()
        .find(|&id| id != view.doc);
    if let Some(alt) = alternate_file {
        cx.editor.switch(alt, Action::Replace);
    } else {
        cx.editor.set_error("no last modified buffer")
    }
}
pub fn goto_first_diag(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);
    let selection = match doc.diagnostics().first() {
        Some(diag) => Selection::single(diag.range.start, diag.range.end),
        None => return,
    };
    push_jump(view, doc);
    doc.set_selection(view.id, selection);
    view.diagnostics_handler
        .immediately_show_diagnostic(doc, view.id);
}

pub fn goto_last_diag(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);
    let selection = match doc.diagnostics().last() {
        Some(diag) => Selection::single(diag.range.start, diag.range.end),
        None => return,
    };
    push_jump(view, doc);
    doc.set_selection(view.id, selection);
    view.diagnostics_handler
        .immediately_show_diagnostic(doc, view.id);
}

pub fn goto_next_diag(cx: &mut Context) {
    let motion = move |editor: &mut Editor| {
        let (view, doc) = current!(editor);

        let cursor_pos = doc
            .selection(view.id)
            .primary()
            .cursor(doc.text().slice(..));

        let diag = doc
            .diagnostics()
            .iter()
            .find(|diag| diag.range.start > cursor_pos);

        let selection = match diag {
            Some(diag) => Selection::single(diag.range.start, diag.range.end),
            None => return,
        };
        push_jump(view, doc);
        doc.set_selection(view.id, selection);
        view.diagnostics_handler
            .immediately_show_diagnostic(doc, view.id);
    };

    cx.editor.apply_motion(motion);
}

pub fn goto_prev_diag(cx: &mut Context) {
    let motion = move |editor: &mut Editor| {
        let (view, doc) = current!(editor);

        let cursor_pos = doc
            .selection(view.id)
            .primary()
            .cursor(doc.text().slice(..));

        let diag = doc
            .diagnostics()
            .iter()
            .rev()
            .find(|diag| diag.range.start < cursor_pos);

        let selection = match diag {
            // NOTE: the selection is reversed because we're jumping to the
            // previous diagnostic.
            Some(diag) => Selection::single(diag.range.end, diag.range.start),
            None => return,
        };
        push_jump(view, doc);
        doc.set_selection(view.id, selection);
        view.diagnostics_handler
            .immediately_show_diagnostic(doc, view.id);
    };
    cx.editor.apply_motion(motion)
}

pub fn goto_first_change(cx: &mut Context) {
    goto_first_change_impl(cx, false);
}

pub fn goto_last_change(cx: &mut Context) {
    goto_first_change_impl(cx, true);
}

pub fn goto_first_change_impl(cx: &mut Context, reverse: bool) {
    let editor = &mut cx.editor;
    let (view, doc) = current!(editor);
    if let Some(handle) = doc.diff_handle() {
        let hunk = {
            let diff = handle.load();
            let idx = if reverse {
                diff.len().saturating_sub(1)
            } else {
                0
            };
            diff.nth_hunk(idx)
        };
        if hunk != Hunk::NONE {
            let range = hunk_range(hunk, doc.text().slice(..));
            push_jump(view, doc);
            doc.set_selection(view.id, Selection::single(range.anchor, range.head));
        }
    }
}

pub fn goto_next_change(cx: &mut Context) {
    goto_next_change_impl(cx, Direction::Forward)
}

pub fn goto_prev_change(cx: &mut Context) {
    goto_next_change_impl(cx, Direction::Backward)
}

pub fn goto_next_change_impl(cx: &mut Context, direction: Direction) {
    let count = cx.count() as u32 - 1;
    let motion = move |editor: &mut Editor| {
        let (view, doc) = current!(editor);
        let doc_text = doc.text().slice(..);
        let diff_handle = if let Some(diff_handle) = doc.diff_handle() {
            diff_handle
        } else {
            editor.set_status("Diff is not available in current buffer");
            return;
        };

        let selection = doc.selection(view.id).clone().transform(|range| {
            let cursor_line = range.cursor_line(doc_text) as u32;

            let diff = diff_handle.load();
            let hunk_idx = match direction {
                Direction::Forward => diff
                    .next_hunk(cursor_line)
                    .map(|idx| (idx + count).min(diff.len() - 1)),
                Direction::Backward => diff
                    .prev_hunk(cursor_line)
                    .map(|idx| idx.saturating_sub(count)),
            };
            let Some(hunk_idx) = hunk_idx else {
                return range;
            };
            let hunk = diff.nth_hunk(hunk_idx);
            let new_range = hunk_range(hunk, doc_text);
            if editor.mode == Mode::Select {
                let head = if new_range.head < range.anchor {
                    new_range.anchor
                } else {
                    new_range.head
                };

                Range::new(range.anchor, head)
            } else {
                new_range.with_direction(direction)
            }
        });

        push_jump(view, doc);
        doc.set_selection(view.id, selection)
    };
    cx.editor.apply_motion(motion);
}
