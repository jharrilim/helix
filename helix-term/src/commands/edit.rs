use super::prelude::*;
use super::{CommentContinuation, Context, helpers::*, open, Open};

pub fn kill_to_line_start(cx: &mut Context) {
    delete_by_selection_insert_mode(
        cx,
        move |text, range| {
            let line = range.cursor_line(text);
            let first_char = text.line_to_char(line);
            let anchor = range.cursor(text);
            let head = if anchor == first_char && line != 0 {
                // select until previous line
                line_end_char_index(&text, line - 1)
            } else if let Some(pos) = text.line(line).first_non_whitespace_char() {
                if first_char + pos < anchor {
                    // select until first non-blank in line if cursor is after it
                    first_char + pos
                } else {
                    // select until start of line
                    first_char
                }
            } else {
                // select until start of line
                first_char
            };
            (head, anchor)
        },
        Direction::Backward,
    );
}

pub fn kill_to_line_end(cx: &mut Context) {
    delete_by_selection_insert_mode(
        cx,
        |text, range| {
            let line = range.cursor_line(text);
            let line_end_pos = line_end_char_index(&text, line);
            let pos = range.cursor(text);

            // if the cursor is on the newline char delete that
            if pos == line_end_pos {
                (pos, text.line_to_char(line + 1))
            } else {
                (pos, line_end_pos)
            }
        },
        Direction::Forward,
    );
}

pub fn goto_first_nonwhitespace(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);

    goto_first_nonwhitespace_impl(
        view,
        doc,
        if cx.editor.mode == Mode::Select {
            Movement::Extend
        } else {
            Movement::Move
        },
    )
}

pub fn extend_to_first_nonwhitespace(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);
    goto_first_nonwhitespace_impl(view, doc, Movement::Extend)
}

pub fn goto_first_nonwhitespace_impl(view: &mut View, doc: &mut Document, movement: Movement) {
    let text = doc.text().slice(..);

    let selection = doc.selection(view.id).clone().transform(|range| {
        let line = range.cursor_line(text);

        if let Some(pos) = text.line(line).first_non_whitespace_char() {
            let pos = pos + text.line_to_char(line);
            range.put_cursor(text, pos, movement == Movement::Extend)
        } else {
            range
        }
    });
    doc.set_selection(view.id, selection);
}

pub fn trim_selections(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);
    let text = doc.text().slice(..);

    let ranges: SmallVec<[Range; 1]> = doc
        .selection(view.id)
        .iter()
        .filter_map(|range| {
            if range.is_empty() || range.slice(text).chars().all(|ch| ch.is_whitespace()) {
                return None;
            }
            let mut start = range.from();
            let mut end = range.to();
            start = movement::skip_while(text, start, |x| x.is_whitespace()).unwrap_or(start);
            end = movement::backwards_skip_while(text, end, |x| x.is_whitespace()).unwrap_or(end);
            Some(Range::new(start, end).with_direction(range.direction()))
        })
        .collect();

    if !ranges.is_empty() {
        let primary = doc.selection(view.id).primary();
        let idx = ranges
            .iter()
            .position(|range| range.overlaps(&primary))
            .unwrap_or(ranges.len() - 1);
        doc.set_selection(view.id, Selection::new(ranges, idx));
    } else {
        collapse_selection(cx);
        keep_primary_selection(cx);
    };
}

// align text in selection
#[allow(deprecated)]
pub fn align_selections(cx: &mut Context) {
    use helix_core::visual_coords_at_pos;

    let (view, doc) = current!(cx.editor);
    let text = doc.text().slice(..);
    let selection = doc.selection(view.id);

    let tab_width = doc.tab_width();

    let mut column_widths: Vec<usize> = Vec::new();
    let mut coordinates = Vec::with_capacity(selection.len());

    let mut previous_line = usize::MAX;
    let mut col_idx = 0;
    let mut running_offset = 0;

    for range in selection {
        let coords = visual_coords_at_pos(text, range.head, tab_width);
        let anchor_coords = visual_coords_at_pos(text, range.anchor, tab_width);

        if coords.row != anchor_coords.row {
            cx.editor
                .set_error("align cannot work with multi line selections");
            return;
        }
        if coords.row != previous_line {
            col_idx = 0;
            running_offset = 0;
            previous_line = coords.row;
        }

        let width = coords.col - running_offset;

        match column_widths.get_mut(col_idx) {
            Some(n) => *n = (*n).max(width),
            None => column_widths.push(width),
        }

        coordinates.push(coords);

        running_offset += width;
        col_idx += 1;
    }

    let column_positions: Vec<_> = column_widths
        .into_iter()
        .scan(0, |sum, n| {
            *sum += n;
            Some(*sum)
        })
        .collect();

    previous_line = usize::MAX;

    let changes = coordinates
        .into_iter()
        .zip(selection)
        .map(|(coords, range)| {
            if coords.row != previous_line {
                col_idx = 0;
                running_offset = 0;
                previous_line = coords.row;
            }
            let current_inserts = column_positions[col_idx] - coords.col - running_offset;
            let insert_pos = range.from();

            col_idx += 1;
            running_offset += current_inserts;

            (
                insert_pos,
                insert_pos,
                Some(" ".repeat(current_inserts).into()),
            )
        });

    let transaction = Transaction::change(doc.text(), changes);
    doc.apply(&transaction, view.id);
    exit_select_mode(cx);
}

pub fn replace(cx: &mut Context) {
    let mut buf = [0u8; 4]; // To hold utf8 encoded char.

    // need to wait for next key
    cx.on_next_key(move |cx, event| {
        let (view, doc) = current!(cx.editor);
        let ch: Option<&str> = match event {
            KeyEvent {
                code: KeyCode::Char(ch),
                ..
            } => Some(ch.encode_utf8(&mut buf[..])),
            KeyEvent {
                code: KeyCode::Enter,
                ..
            } => Some(doc.line_ending.as_str()),
            KeyEvent {
                code: KeyCode::Tab, ..
            } => Some("\t"),
            _ => None,
        };

        let selection = doc.selection(view.id);

        if let Some(ch) = ch {
            let transaction = Transaction::change_by_selection(doc.text(), selection, |range| {
                if !range.is_empty() {
                    let text: Tendril = doc
                        .text()
                        .slice(range.from()..range.to())
                        .graphemes()
                        .map(|_g| ch)
                        .collect();
                    (range.from(), range.to(), Some(text))
                } else {
                    // No change.
                    (range.from(), range.to(), None)
                }
            });

            doc.apply(&transaction, view.id);
            exit_select_mode(cx);
        }
    })
}

pub fn switch_case_impl<F>(cx: &mut Context, change_fn: F)
where
    F: Fn(RopeSlice) -> Tendril,
{
    let (view, doc) = current!(cx.editor);
    let selection = doc.selection(view.id);
    let transaction = Transaction::change_by_selection(doc.text(), selection, |range| {
        let text: Tendril = change_fn(range.slice(doc.text().slice(..)));

        (range.from(), range.to(), Some(text))
    });

    doc.apply(&transaction, view.id);
    exit_select_mode(cx);
}

enum CaseSwitcher {
    Upper(ToUppercase),
    Lower(ToLowercase),
    Keep(Option<char>),
}

impl Iterator for CaseSwitcher {
    type Item = char;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            CaseSwitcher::Upper(upper) => upper.next(),
            CaseSwitcher::Lower(lower) => lower.next(),
            CaseSwitcher::Keep(ch) => ch.take(),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            CaseSwitcher::Upper(upper) => upper.size_hint(),
            CaseSwitcher::Lower(lower) => lower.size_hint(),
            CaseSwitcher::Keep(ch) => {
                let n = if ch.is_some() { 1 } else { 0 };
                (n, Some(n))
            }
        }
    }
}

impl ExactSizeIterator for CaseSwitcher {}

pub fn switch_case(cx: &mut Context) {
    switch_case_impl(cx, |string| {
        string
            .chars()
            .flat_map(|ch| {
                if ch.is_lowercase() {
                    CaseSwitcher::Upper(ch.to_uppercase())
                } else if ch.is_uppercase() {
                    CaseSwitcher::Lower(ch.to_lowercase())
                } else {
                    CaseSwitcher::Keep(Some(ch))
                }
            })
            .collect()
    });
}

pub fn switch_to_uppercase(cx: &mut Context) {
    switch_case_impl(cx, |string| {
        string.chunks().map(|chunk| chunk.to_uppercase()).collect()
    });
}

pub fn switch_to_lowercase(cx: &mut Context) {
    switch_case_impl(cx, |string| {
        string.chunks().map(|chunk| chunk.to_lowercase()).collect()
    });
}
enum Operation {
    Delete,
    Change,
}

pub fn selection_is_linewise(selection: &Selection, text: &Rope) -> bool {
    selection.ranges().iter().all(|range| {
        let text = text.slice(..);
        if range.slice(text).len_lines() < 2 {
            return false;
        }
        // If the start of the selection is at the start of a line and the end at the end of a line.
        let (start_line, end_line) = range.line_range(text);
        let start = text.line_to_char(start_line);
        let end = text.line_to_char((end_line + 1).min(text.len_lines()));
        start == range.from() && end == range.to()
    })
}

enum YankAction {
    Yank,
    NoYank,
}

pub fn delete_selection_impl(cx: &mut Context, op: Operation, yank: YankAction) {
    let (view, doc) = current!(cx.editor);

    let selection = doc.selection(view.id);
    let only_whole_lines = selection_is_linewise(selection, doc.text());

    if cx.register != Some('_') && matches!(yank, YankAction::Yank) {
        // yank the selection
        let text = doc.text().slice(..);
        let values: Vec<String> = selection.fragments(text).map(Cow::into_owned).collect();
        let reg_name = cx
            .register
            .unwrap_or_else(|| cx.editor.config.load().default_yank_register);
        if let Err(err) = cx.editor.registers.write(reg_name, values) {
            cx.editor.set_error(err.to_string());
            return;
        }
    }

    // delete the selection
    let transaction =
        Transaction::delete_by_selection(doc.text(), selection, |range| (range.from(), range.to()));
    doc.apply(&transaction, view.id);

    match op {
        Operation::Delete => {
            // exit select mode, if currently in select mode
            exit_select_mode(cx);
        }
        Operation::Change => {
            if only_whole_lines {
                open(cx, Open::Above, CommentContinuation::Disabled);
            } else {
                enter_insert_mode(cx);
            }
        }
    }
}

#[inline]
pub fn delete_by_selection_insert_mode(
    cx: &mut Context,
    mut f: impl FnMut(RopeSlice, &Range) -> Deletion,
    direction: Direction,
) {
    let (view, doc) = current!(cx.editor);
    let text = doc.text().slice(..);
    let mut selection = SmallVec::new();
    let mut insert_newline = false;
    let text_len = text.len_chars();
    let mut transaction =
        Transaction::delete_by_selection(doc.text(), doc.selection(view.id), |range| {
            let (start, end) = f(text, range);
            if direction == Direction::Forward {
                let mut range = *range;
                if range.head > range.anchor {
                    insert_newline |= end == text_len;
                    // move the cursor to the right so that the selection
                    // doesn't shrink when deleting forward (so the text appears to
                    // move to  left)
                    // += 1 is enough here as the range is normalized to grapheme boundaries
                    // later anyway
                    range.head += 1;
                }
                selection.push(range);
            }
            (start, end)
        });

    // in case we delete the last character and the cursor would be moved to the EOF char
    // insert a newline, just like when entering append mode
    if insert_newline {
        transaction = transaction.insert_at_eof(doc.line_ending.as_str().into());
    }

    if direction == Direction::Forward {
        doc.set_selection(
            view.id,
            Selection::new(selection, doc.selection(view.id).primary_index()),
        );
    }
    doc.apply(&transaction, view.id);
}

pub fn delete_selection(cx: &mut Context) {
    delete_selection_impl(cx, Operation::Delete, YankAction::Yank);
}

pub fn delete_selection_noyank(cx: &mut Context) {
    delete_selection_impl(cx, Operation::Delete, YankAction::NoYank);
}

pub fn change_selection(cx: &mut Context) {
    delete_selection_impl(cx, Operation::Change, YankAction::Yank);
}

pub fn change_selection_noyank(cx: &mut Context) {
    delete_selection_impl(cx, Operation::Change, YankAction::NoYank);
}

pub fn collapse_selection(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);
    let text = doc.text().slice(..);

    let selection = doc.selection(view.id).clone().transform(|range| {
        let pos = range.cursor(text);
        Range::new(pos, pos)
    });
    doc.set_selection(view.id, selection);
}

pub fn flip_selections(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);

    let selection = doc
        .selection(view.id)
        .clone()
        .transform(|range| range.flip());
    doc.set_selection(view.id, selection);
}

pub fn ensure_selections_forward(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);

    let selection = doc
        .selection(view.id)
        .clone()
        .transform(|r| r.with_direction(Direction::Forward));

    doc.set_selection(view.id, selection);
}

enum IndentFallbackPos {
    LineStart,
    LineEnd,
}

pub fn insert_at_line_start(cx: &mut Context) {
    insert_with_indent(cx, IndentFallbackPos::LineStart);
}

// `A` inserts at the end of each line with a selection.
// If the line is empty, automatically indent.
pub fn insert_at_line_end(cx: &mut Context) {
    insert_with_indent(cx, IndentFallbackPos::LineEnd);
}

// Enter insert mode and auto-indent the current line if it is empty.
// If the line is not empty, move the cursor to the specified fallback position.
pub fn insert_with_indent(cx: &mut Context, cursor_fallback: IndentFallbackPos) {
    enter_insert_mode(cx);

    let (view, doc) = current!(cx.editor);
    let loader = cx.editor.syn_loader.load();

    let text = doc.text().slice(..);
    let contents = doc.text();
    let selection = doc.selection(view.id);

    let syntax = doc.syntax();
    let tab_width = doc.tab_width();

    let mut ranges = SmallVec::with_capacity(selection.len());
    let mut offs = 0;

    let mut transaction = Transaction::change_by_selection(contents, selection, |range| {
        let cursor_line = range.cursor_line(text);
        let cursor_line_start = text.line_to_char(cursor_line);

        if line_end_char_index(&text, cursor_line) == cursor_line_start {
            // line is empty => auto indent
            let line_end_index = cursor_line_start;

            let indent = indent::indent_for_newline(
                &loader,
                syntax,
                &doc.config.load().indent_heuristic,
                &doc.indent_style,
                tab_width,
                text,
                cursor_line,
                line_end_index,
                cursor_line,
            );

            // calculate new selection ranges
            let pos = offs + cursor_line_start;
            let indent_width = indent.chars().count();
            ranges.push(Range::point(pos + indent_width));
            offs += indent_width;

            (line_end_index, line_end_index, Some(indent.into()))
        } else {
            // move cursor to the fallback position
            let pos = match cursor_fallback {
                IndentFallbackPos::LineStart => text
                    .line(cursor_line)
                    .first_non_whitespace_char()
                    .map(|ws_offset| ws_offset + cursor_line_start)
                    .unwrap_or(cursor_line_start),
                IndentFallbackPos::LineEnd => line_end_char_index(&text, cursor_line),
            };

            ranges.push(range.put_cursor(text, pos + offs, cx.editor.mode == Mode::Select));

            (cursor_line_start, cursor_line_start, None)
        }
    });

    transaction = transaction.with_selection(Selection::new(ranges, selection.primary_index()));
    doc.apply(&transaction, view.id);
}

// Creates an LspCallback that waits for formatting changes to be computed. When they're done,
// it applies them, but only if the doc hasn't changed.
//
// TODO: provide some way to cancel this, probably as part of a more general job cancellation
// scheme
pub(crate) async fn make_format_callback(
    doc_id: DocumentId,
    doc_version: i32,
    view_id: ViewId,
    format: impl Future<Output = Result<Transaction, FormatterError>> + Send + 'static,
    write: Option<(Option<PathBuf>, bool)>,
) -> anyhow::Result<job::Callback> {
    let format = format.await;

    let call: job::Callback = Callback::Editor(Box::new(move |editor| {
        if !editor.documents.contains_key(&doc_id) || !editor.tree.contains(view_id) {
            return;
        }

        let scrolloff = editor.config().scrolloff;
        let doc = doc_mut!(editor, &doc_id);
        let view = view_mut!(editor, view_id);

        match format {
            Ok(format) => {
                if doc.version() == doc_version {
                    doc.apply(&format, view.id);
                    doc.append_changes_to_history(view);
                    doc.detect_indent_and_line_ending();
                    view.ensure_cursor_in_view(doc, scrolloff);
                } else {
                    log::info!("discarded formatting changes because the document changed");
                }
            }
            Err(err) => {
                if write.is_none() {
                    editor.set_error(err.to_string());
                    return;
                }
                log::info!("failed to format '{}': {err}", doc.display_name());
            }
        }

        if let Some((path, force)) = write {
            let id = doc.id();
            if let Err(err) = editor.save(id, path, force) {
                editor.set_error(format!("Error saving: {}", err));
            }
        }
    }));

    Ok(call)
}

fn get_lines(doc: &Document, view_id: ViewId) -> Vec<usize> {
    let mut lines = Vec::new();

    for range in doc.selection(view_id) {
        let (start, end) = range.line_range(doc.text().slice(..));

        for line in start..=end {
            lines.push(line)
        }
    }
    lines.sort_unstable();
    lines.dedup();
    lines
}

pub fn indent(cx: &mut Context) {
    let count = cx.count();
    let (view, doc) = current!(cx.editor);
    let lines = get_lines(doc, view.id);

    // Indent by one level
    let indent = Tendril::from(doc.indent_style.as_str().repeat(count));

    let transaction = Transaction::change(
        doc.text(),
        lines.into_iter().filter_map(|line| {
            let is_blank = doc.text().line(line).chunks().all(|s| s.trim().is_empty());
            if is_blank {
                return None;
            }
            let pos = doc.text().line_to_char(line);

            let indent = if let IndentStyle::Spaces(indent_width) = doc.indent_style {
                let line = doc.text().line(line);
                let offset = line.first_non_whitespace_char().unwrap_or(0) % indent_width as usize;
                indent.clone().split_off(offset)
            } else {
                indent.clone()
            };

            Some((pos, pos, Some(indent)))
        }),
    );
    doc.apply(&transaction, view.id);
    exit_select_mode(cx);
}

pub fn unindent(cx: &mut Context) {
    let count = cx.count();
    let (view, doc) = current!(cx.editor);
    let lines = get_lines(doc, view.id);
    let mut changes = Vec::with_capacity(lines.len());
    let tab_width = doc.tab_width();
    let indent_width = count * doc.indent_width();

    for line_idx in lines {
        let line = doc.text().line(line_idx);
        let mut width = 0;
        let mut pos = 0;

        for ch in line.chars() {
            match ch {
                ' ' => width += 1,
                '\t' => width = (width / tab_width + 1) * tab_width,
                _ => break,
            }

            pos += 1;

            if width >= indent_width {
                break;
            }
        }

        // now delete from start to first non-blank
        if pos > 0 {
            let start = doc.text().line_to_char(line_idx);
            changes.push((start, start + pos, None))
        }
    }

    let transaction = Transaction::change(doc.text(), changes.into_iter());

    doc.apply(&transaction, view.id);
    exit_select_mode(cx);
}

pub fn format_selections(cx: &mut Context) {
    use helix_lsp::{lsp, util::range_to_lsp_range};

    let (view, doc) = current!(cx.editor);
    let view_id = view.id;

    // via lsp if available
    // TODO: else via tree-sitter indentation calculations

    if doc.selection(view_id).len() != 1 {
        cx.editor
            .set_error("format_selections only supports a single selection for now");
        return;
    }

    // TODO extra LanguageServerFeature::FormatSelections?
    // maybe such that LanguageServerFeature::Format contains it as well
    let Some(language_server) = doc
        .language_servers_with_feature(LanguageServerFeature::Format)
        .find(|ls| {
            matches!(
                ls.capabilities().document_range_formatting_provider,
                Some(lsp::OneOf::Left(true) | lsp::OneOf::Right(_))
            )
        })
    else {
        cx.editor
            .set_error("No configured language server supports range formatting");
        return;
    };

    let offset_encoding = language_server.offset_encoding();
    let ranges: Vec<lsp::Range> = doc
        .selection(view_id)
        .iter()
        .map(|range| range_to_lsp_range(doc.text(), *range, offset_encoding))
        .collect();

    // TODO: handle fails
    // TODO: concurrent map over all ranges

    let range = ranges[0];

    let future = language_server
        .text_document_range_formatting(
            doc.identifier(),
            range,
            lsp::FormattingOptions {
                tab_size: doc.tab_width() as u32,
                insert_spaces: matches!(doc.indent_style, IndentStyle::Spaces(_)),
                ..Default::default()
            },
            None,
        )
        .unwrap();

    let text = doc.text().clone();
    let doc_id = doc.id();
    let doc_version = doc.version();

    tokio::spawn(async move {
        match future.await {
            Ok(Some(res)) => {
                let transaction =
                    helix_lsp::util::generate_transaction_from_edits(&text, res, offset_encoding);
                job::dispatch(move |editor, _compositor| {
                    let Some(doc) = editor.document_mut(doc_id) else {
                        return;
                    };
                    // Updating a desynced document causes problems with applying the transaction
                    if doc.version() != doc_version {
                        return;
                    }
                    doc.apply(&transaction, view_id);
                })
                .await
            }
            Err(err) => log::error!("format sections failed: {err}"),
            Ok(None) => (),
        }
    });
}

pub fn join_selections_impl(cx: &mut Context, select_space: bool) {
    use movement::skip_while;
    let (view, doc) = current!(cx.editor);
    let text = doc.text();
    let slice = text.slice(..);

    let comment_tokens = doc
        .language_config()
        .and_then(|config| config.comment_tokens.as_deref())
        .unwrap_or(&[]);
    // Sort by length to handle Rust's /// vs //
    let mut comment_tokens: Vec<&str> = comment_tokens.iter().map(|x| x.as_str()).collect();
    comment_tokens.sort_unstable_by_key(|x| std::cmp::Reverse(x.len()));

    let mut changes = Vec::new();

    for selection in doc.selection(view.id) {
        let (start, mut end) = selection.line_range(slice);
        if start == end {
            end = (end + 1).min(text.len_lines() - 1);
        }
        let lines = start..end;

        changes.reserve(lines.len());

        let first_line_idx = slice.line_to_char(start);
        let first_line_idx = skip_while(slice, first_line_idx, |ch| matches!(ch, ' ' | '\t'))
            .unwrap_or(first_line_idx);
        let first_line = slice.slice(first_line_idx..);
        let mut current_comment_token = comment_tokens
            .iter()
            .find(|token| first_line.starts_with(token));

        for line in lines {
            let start = line_end_char_index(&slice, line);
            let mut end = text.line_to_char(line + 1);
            end = skip_while(slice, end, |ch| matches!(ch, ' ' | '\t')).unwrap_or(end);
            let slice_from_end = slice.slice(end..);
            if let Some(token) = comment_tokens
                .iter()
                .find(|token| slice_from_end.starts_with(token))
            {
                if Some(token) == current_comment_token {
                    end += token.chars().count();
                    end = skip_while(slice, end, |ch| matches!(ch, ' ' | '\t')).unwrap_or(end);
                } else {
                    // update current token, but don't delete this one.
                    current_comment_token = Some(token);
                }
            }

            let separator = if end == line_end_char_index(&slice, line + 1) {
                // the joining line contains only space-characters => don't include a whitespace when joining
                None
            } else {
                Some(Tendril::from(" "))
            };
            changes.push((start, end, separator));
        }
    }

    // nothing to do, bail out early to avoid crashes later
    if changes.is_empty() {
        return;
    }

    changes.sort_unstable_by_key(|(from, _to, _text)| *from);
    changes.dedup();

    // select inserted spaces
    let transaction = if select_space {
        let mut offset: usize = 0;
        let ranges: SmallVec<_> = changes
            .iter()
            .filter_map(|change| {
                if change.2.is_some() {
                    let range = Range::point(change.0 - offset);
                    offset += change.1 - change.0 - 1; // -1 adjusts for the replacement of the range by a space
                    Some(range)
                } else {
                    offset += change.1 - change.0;
                    None
                }
            })
            .collect();
        let t = Transaction::change(text, changes.into_iter());
        if ranges.is_empty() {
            t
        } else {
            let selection = Selection::new(ranges, 0);
            t.with_selection(selection)
        }
    } else {
        Transaction::change(text, changes.into_iter())
    };

    doc.apply(&transaction, view.id);
}

pub fn keep_or_remove_selections_impl(cx: &mut Context, remove: bool) {
    // keep or remove selections matching regex
    let reg = cx.register.unwrap_or('/');
    ui::regex_prompt(
        cx,
        if remove { "remove:" } else { "keep:" }.into(),
        Some(reg),
        ui::completers::none,
        move |cx, regex, event| {
            let (view, doc) = current!(cx.editor);
            if !matches!(event, PromptEvent::Update | PromptEvent::Validate) {
                return;
            }
            let text = doc.text().slice(..);

            if let Some(selection) =
                selection::keep_or_remove_matches(text, doc.selection(view.id), &regex, remove)
            {
                doc.set_selection(view.id, selection);
            } else if event == PromptEvent::Validate {
                cx.editor.set_error("no selections remaining");
            }
        },
    )
}

pub fn join_selections(cx: &mut Context) {
    join_selections_impl(cx, false)
}

pub fn join_selections_space(cx: &mut Context) {
    join_selections_impl(cx, true)
}

pub fn keep_selections(cx: &mut Context) {
    keep_or_remove_selections_impl(cx, false)
}

pub fn remove_selections(cx: &mut Context) {
    keep_or_remove_selections_impl(cx, true)
}

pub fn keep_primary_selection(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);
    // TODO: handle count

    let range = doc.selection(view.id).primary();
    doc.set_selection(view.id, Selection::single(range.anchor, range.head));
}

pub fn remove_primary_selection(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);
    // TODO: handle count

    let selection = doc.selection(view.id);
    if selection.len() == 1 {
        cx.editor.set_error("no selections remaining");
        return;
    }
    let index = selection.primary_index();
    let selection = selection.clone().remove(index);

    doc.set_selection(view.id, selection);
}

pub fn completion(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);
    let range = doc.selection(view.id).primary();
    let text = doc.text().slice(..);
    let cursor = range.cursor(text);

    cx.editor
        .handlers
        .trigger_completions(cursor, doc.id(), view.id);
}

// comments
type CommentTransactionFn = fn(
    line_token: Option<&str>,
    block_tokens: Option<&[BlockCommentToken]>,
    doc: &Rope,
    selection: &Selection,
) -> Transaction;

pub fn toggle_comments_impl(cx: &mut Context, comment_transaction: CommentTransactionFn) {
    let (view, doc) = current!(cx.editor);
    let line_token: Option<&str> = doc
        .language_config()
        .and_then(|lc| lc.comment_tokens.as_ref())
        .and_then(|tc| tc.first())
        .map(|tc| tc.as_str());
    let block_tokens: Option<&[BlockCommentToken]> = doc
        .language_config()
        .and_then(|lc| lc.block_comment_tokens.as_ref())
        .map(|tc| &tc[..]);

    let transaction =
        comment_transaction(line_token, block_tokens, doc.text(), doc.selection(view.id));

    doc.apply(&transaction, view.id);
    exit_select_mode(cx);
}

/// commenting behavior:
/// 1. only line comment tokens -> line comment
/// 2. each line block commented -> uncomment all lines
/// 3. whole selection block commented -> uncomment selection
/// 4. all lines not commented and block tokens -> comment uncommented lines
/// 5. no comment tokens and not block commented -> line comment
pub fn toggle_comments(cx: &mut Context) {
    toggle_comments_impl(cx, |line_token, block_tokens, doc, selection| {
        let text = doc.slice(..);

        // only have line comment tokens
        if line_token.is_some() && block_tokens.is_none() {
            return comment::toggle_line_comments(doc, selection, line_token);
        }

        let split_lines = comment::split_lines_of_selection(text, selection);

        let default_block_tokens = &[BlockCommentToken::default()];
        let block_comment_tokens = block_tokens.unwrap_or(default_block_tokens);

        let (line_commented, line_comment_changes) =
            comment::find_block_comments(block_comment_tokens, text, &split_lines);

        // block commented by line would also be block commented so check this first
        if line_commented {
            return comment::create_block_comment_transaction(
                doc,
                &split_lines,
                line_commented,
                line_comment_changes,
            )
            .0;
        }

        let (block_commented, comment_changes) =
            comment::find_block_comments(block_comment_tokens, text, selection);

        // check if selection has block comments
        if block_commented {
            return comment::create_block_comment_transaction(
                doc,
                selection,
                block_commented,
                comment_changes,
            )
            .0;
        }

        // not commented and only have block comment tokens
        if line_token.is_none() && block_tokens.is_some() {
            return comment::create_block_comment_transaction(
                doc,
                &split_lines,
                line_commented,
                line_comment_changes,
            )
            .0;
        }

        // not block commented at all and don't have any tokens
        comment::toggle_line_comments(doc, selection, line_token)
    })
}

pub fn toggle_line_comments(cx: &mut Context) {
    toggle_comments_impl(cx, |line_token, block_tokens, doc, selection| {
        if line_token.is_none() && block_tokens.is_some() {
            let default_block_tokens = &[BlockCommentToken::default()];
            let block_comment_tokens = block_tokens.unwrap_or(default_block_tokens);
            comment::toggle_block_comments(
                doc,
                &comment::split_lines_of_selection(doc.slice(..), selection),
                block_comment_tokens,
            )
        } else {
            comment::toggle_line_comments(doc, selection, line_token)
        }
    });
}

pub fn toggle_block_comments(cx: &mut Context) {
    toggle_comments_impl(cx, |line_token, block_tokens, doc, selection| {
        if line_token.is_some() && block_tokens.is_none() {
            comment::toggle_line_comments(doc, selection, line_token)
        } else {
            let default_block_tokens = &[BlockCommentToken::default()];
            let block_comment_tokens = block_tokens.unwrap_or(default_block_tokens);
            comment::toggle_block_comments(doc, selection, block_comment_tokens)
        }
    });
}
pub fn add_newline_above(cx: &mut Context) {
    add_newline_impl(cx, Open::Above);
}

pub fn add_newline_below(cx: &mut Context) {
    add_newline_impl(cx, Open::Below)
}

pub fn add_newline_impl(cx: &mut Context, open: Open) {
    let count = cx.count();
    let (view, doc) = current!(cx.editor);
    let selection = doc.selection(view.id);
    let text = doc.text();
    let slice = text.slice(..);

    let changes = selection.into_iter().map(|range| {
        let (start, end) = range.line_range(slice);
        let line = match open {
            Open::Above => start,
            Open::Below => end + 1,
        };
        let pos = text.line_to_char(line);
        (
            pos,
            pos,
            Some(doc.line_ending.as_str().repeat(count).into()),
        )
    });

    let transaction = Transaction::change(text, changes);
    doc.apply(&transaction, view.id);
}
