use super::prelude::*;
use super::{
    insert_char, lsp, scroll, syntax, syntax_symbol_picker, syntax_workspace_symbol_picker,
    Context, helpers::*,
};

pub fn insert_mode(cx: &mut Context) {
    enter_insert_mode(cx);
    let (view, doc) = current!(cx.editor);

    log::trace!(
        "entering insert mode with sel: {:?}, text: {:?}",
        doc.selection(view.id),
        doc.text().to_string()
    );

    let selection = doc
        .selection(view.id)
        .clone()
        .transform(|range| Range::new(range.to(), range.from()));

    doc.set_selection(view.id, selection);
}

// inserts at the end of each selection
pub fn append_mode(cx: &mut Context) {
    enter_insert_mode(cx);
    let (view, doc) = current!(cx.editor);
    doc.restore_cursor = true;
    let text = doc.text().slice(..);

    // Make sure there's room at the end of the document if the last
    // selection butts up against it.
    let end = text.len_chars();
    let last_range = doc
        .selection(view.id)
        .iter()
        .last()
        .expect("selection should always have at least one range");
    if !last_range.is_empty() && last_range.to() == end {
        let transaction = Transaction::change(
            doc.text(),
            [(end, end, Some(doc.line_ending.as_str().into()))].into_iter(),
        );
        doc.apply(&transaction, view.id);
    }

    let selection = doc.selection(view.id).clone().transform(|range| {
        Range::new(
            range.from(),
            graphemes::next_grapheme_boundary(doc.text().slice(..), range.to()),
        )
    });
    doc.set_selection(view.id, selection);
}
pub fn normal_mode(cx: &mut Context) {
    cx.editor.enter_normal_mode();
}
pub fn select_mode(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);
    let text = doc.text().slice(..);

    // Make sure end-of-document selections are also 1-width.
    // (With the exception of being in an empty document, of course.)
    let selection = doc.selection(view.id).clone().transform(|range| {
        if range.is_empty() && range.head == text.len_chars() {
            Range::new(
                graphemes::prev_grapheme_boundary(text, range.anchor),
                range.head,
            )
        } else {
            range
        }
    });
    doc.set_selection(view.id, selection);

    cx.editor.mode = Mode::Select;
}

pub fn suspend(_cx: &mut Context) {
    #[cfg(not(windows))]
    {
        // SAFETY: These are calls to standard POSIX functions.
        // Unsafe is necessary since we are calling outside of Rust.
        let is_session_leader = unsafe { libc::getpid() == libc::getsid(0) };

        // If helix is the session leader, there is nothing to suspend to, so skip
        if is_session_leader {
            return;
        }
        _cx.block_try_flush_writes().ok();
        signal_hook::low_level::raise(signal_hook::consts::signal::SIGTSTP).unwrap();
    }
}

enum IncrementDirection {
    Increase,
    Decrease,
}

/// Increment objects within selections by count.
pub fn increment(cx: &mut Context) {
    increment_impl(cx, IncrementDirection::Increase);
}

/// Decrement objects within selections by count.
pub fn decrement(cx: &mut Context) {
    increment_impl(cx, IncrementDirection::Decrease);
}

/// Increment objects within selections by `amount`.
/// A negative `amount` will decrement objects within selections.
pub fn increment_impl(cx: &mut Context, increment_direction: IncrementDirection) {
    let sign = match increment_direction {
        IncrementDirection::Increase => 1,
        IncrementDirection::Decrease => -1,
    };
    let mut amount = sign * cx.count() as i64;
    // If the register is `#` then increase or decrease the `amount` by 1 per element
    let increase_by = if cx.register == Some('#') { sign } else { 0 };

    let (view, doc) = current!(cx.editor);
    let selection = doc.selection(view.id);
    let text = doc.text().slice(..);

    let mut new_selection_ranges = SmallVec::new();
    let mut cumulative_length_diff: i128 = 0;
    let mut changes = vec![];

    for range in selection {
        let selected_text: Cow<str> = range.fragment(text);
        let new_from = ((range.from() as i128) + cumulative_length_diff) as usize;
        let incremented = [increment::integer, increment::date_time]
            .iter()
            .find_map(|incrementor| incrementor(selected_text.as_ref(), amount));

        amount += increase_by;

        match incremented {
            None => {
                let new_range = Range::new(
                    new_from,
                    (range.to() as i128 + cumulative_length_diff) as usize,
                );
                new_selection_ranges.push(new_range);
            }
            Some(new_text) => {
                let new_range = Range::new(new_from, new_from + new_text.len());
                cumulative_length_diff += new_text.len() as i128 - selected_text.len() as i128;
                new_selection_ranges.push(new_range);
                changes.push((range.from(), range.to(), Some(new_text.into())));
            }
        }
    }

    if !changes.is_empty() {
        let new_selection = Selection::new(new_selection_ranges, selection.primary_index());
        let transaction = Transaction::change(doc.text(), changes.into_iter());
        let transaction = transaction.with_selection(new_selection);
        doc.apply(&transaction, view.id);
        exit_select_mode(cx);
    }
}

pub fn goto_next_tabstop(cx: &mut Context) {
    goto_next_tabstop_impl(cx, Direction::Forward)
}

pub fn goto_prev_tabstop(cx: &mut Context) {
    goto_next_tabstop_impl(cx, Direction::Backward)
}

pub fn goto_next_tabstop_impl(cx: &mut Context, direction: Direction) {
    let (view, doc) = current!(cx.editor);
    let view_id = view.id;
    let Some(mut snippet) = doc.active_snippet.take() else {
        cx.editor.set_error("no snippet is currently active");
        return;
    };
    let tabstop = match direction {
        Direction::Forward => Some(snippet.next_tabstop(doc.selection(view_id))),
        Direction::Backward => snippet
            .prev_tabstop(doc.selection(view_id))
            .map(|selection| (selection, false)),
    };
    let Some((selection, last_tabstop)) = tabstop else {
        return;
    };
    doc.set_selection(view_id, selection);
    if !last_tabstop {
        doc.active_snippet = Some(snippet)
    }
    if cx.editor.mode() == Mode::Insert {
        cx.on_next_key_fallback(|cx, key| {
            if let Some(c) = key.char() {
                let (view, doc) = current!(cx.editor);
                if let Some(snippet) = &doc.active_snippet {
                    doc.apply(&snippet.delete_placeholder(doc.text()), view.id);
                }
                insert_char(cx, c);
            }
        })
    }
}

pub fn record_macro(cx: &mut Context) {
    if let Some((reg, mut keys)) = cx.editor.macro_recording.take() {
        // Remove the keypress which ends the recording
        keys.pop();
        let s = keys
            .into_iter()
            .map(|key| {
                let s = key.to_string();
                if s.chars().count() == 1 {
                    s
                } else {
                    format!("<{}>", s)
                }
            })
            .collect::<String>();
        match cx.editor.registers.write(reg, vec![s]) {
            Ok(_) => cx
                .editor
                .set_status(format!("Recorded to register [{}]", reg)),
            Err(err) => cx.editor.set_error(err.to_string()),
        }
    } else {
        let reg = cx.register.take().unwrap_or('@');
        cx.editor.macro_recording = Some((reg, Vec::new()));
        cx.editor
            .set_status(format!("Recording to register [{}]", reg));
    }
}

pub fn replay_macro(cx: &mut Context) {
    let reg = cx.register.unwrap_or('@');

    if cx.editor.macro_replaying.contains(&reg) {
        cx.editor.set_error(format!(
            "Cannot replay from register [{}] because already replaying from same register",
            reg
        ));
        return;
    }

    let keys: Vec<KeyEvent> = if let Some(keys) = cx
        .editor
        .registers
        .read(reg, cx.editor)
        .filter(|values| values.len() == 1)
        .map(|mut values| values.next().unwrap())
    {
        match helix_view::input::parse_macro(&keys) {
            Ok(keys) => keys,
            Err(err) => {
                cx.editor.set_error(format!("Invalid macro: {}", err));
                return;
            }
        }
    } else {
        cx.editor.set_error(format!("Register [{}] empty", reg));
        return;
    };

    // Once the macro has been fully validated, it's marked as being under replay
    // to ensure we don't fall into infinite recursion.
    cx.editor.macro_replaying.push(reg);

    let count = cx.count();
    cx.callback.push(Box::new(move |compositor, cx| {
        for _ in 0..count {
            for &key in keys.iter() {
                compositor.handle_event(&compositor::Event::Key(key), cx);
            }
        }
        // The macro under replay is cleared at the end of the callback, not in the
        // macro replay context, or it will not correctly protect the user from
        // replaying recursively.
        cx.editor.macro_replaying.pop();
    }));
}

pub fn goto_word(cx: &mut Context) {
    jump_to_word(cx, Movement::Move)
}

pub fn extend_to_word(cx: &mut Context) {
    jump_to_word(cx, Movement::Extend)
}

pub fn jump_to_label(cx: &mut Context, labels: Vec<Range>, behaviour: Movement) {
    let doc = doc!(cx.editor);
    let alphabet = &cx.editor.config().jump_label_alphabet;
    if labels.is_empty() {
        return;
    }
    let alphabet_char = |i| {
        let mut res = Tendril::new();
        res.push(alphabet[i]);
        res
    };

    // Add label for each jump candidate to the View as virtual text.
    let text = doc.text().slice(..);
    let mut overlays: Vec<_> = labels
        .iter()
        .enumerate()
        .flat_map(|(i, range)| {
            [
                Overlay::new(range.from(), alphabet_char(i / alphabet.len())),
                Overlay::new(
                    graphemes::next_grapheme_boundary(text, range.from()),
                    alphabet_char(i % alphabet.len()),
                ),
            ]
        })
        .collect();
    overlays.sort_unstable_by_key(|overlay| overlay.char_idx);
    let (view, doc) = current!(cx.editor);
    doc.set_jump_labels(view.id, overlays);

    // Accept two characters matching a visible label. Jump to the candidate
    // for that label if it exists.
    let primary_selection = doc.selection(view.id).primary();
    let view_id = view.id;
    let doc = doc.id();
    cx.on_next_key(move |cx, event| {
        let alphabet = &cx.editor.config().jump_label_alphabet;
        let Some(i) = event
            .char()
            .filter(|_| event.modifiers.is_empty())
            .and_then(|ch| alphabet.iter().position(|&it| it == ch))
        else {
            doc_mut!(cx.editor, &doc).remove_jump_labels(view_id);
            return;
        };
        let outer = i * alphabet.len();
        // Bail if the given character cannot be a jump label.
        if outer > labels.len() {
            doc_mut!(cx.editor, &doc).remove_jump_labels(view_id);
            return;
        }
        cx.on_next_key(move |cx, event| {
            doc_mut!(cx.editor, &doc).remove_jump_labels(view_id);
            let alphabet = &cx.editor.config().jump_label_alphabet;
            let Some(inner) = event
                .char()
                .filter(|_| event.modifiers.is_empty())
                .and_then(|ch| alphabet.iter().position(|&it| it == ch))
            else {
                return;
            };
            if let Some(mut range) = labels.get(outer + inner).copied() {
                range = if behaviour == Movement::Extend {
                    let anchor = if range.anchor < range.head {
                        let from = primary_selection.from();
                        if range.anchor < from {
                            range.anchor
                        } else {
                            from
                        }
                    } else {
                        let to = primary_selection.to();
                        if range.anchor > to {
                            range.anchor
                        } else {
                            to
                        }
                    };
                    Range::new(anchor, range.head)
                } else {
                    range.with_direction(Direction::Forward)
                };
                let (view, doc) = current!(cx.editor);
                push_jump(view, doc);
                doc.set_selection(view_id, range.into());
            }
        });
    });
}

pub fn jump_to_word(cx: &mut Context, behaviour: Movement) {
    // Calculate the jump candidates: ranges for any visible words with two or
    // more characters.
    let alphabet = &cx.editor.config().jump_label_alphabet;
    if alphabet.is_empty() {
        return;
    }

    let jump_label_limit = alphabet.len() * alphabet.len();
    let mut words = Vec::with_capacity(jump_label_limit);
    let (view, doc) = current_ref!(cx.editor);
    let text = doc.text().slice(..);

    // This is not necessarily exact if there is virtual text like soft wrap.
    // It's ok though because the extra jump labels will not be rendered.
    let start = text.line_to_char(text.char_to_line(doc.view_offset(view.id).anchor));
    let end = text.line_to_char(view.estimate_last_doc_line(doc) + 1);

    let primary_selection = doc.selection(view.id).primary();
    let cursor = primary_selection.cursor(text);
    let mut cursor_fwd = Range::point(cursor);
    let mut cursor_rev = Range::point(cursor);
    if text.get_char(cursor).is_some_and(|c| !c.is_whitespace()) {
        let cursor_word_end = movement::move_next_word_end(text, cursor_fwd, 1);
        //  single grapheme words need a special case
        if cursor_word_end.anchor == cursor {
            cursor_fwd = cursor_word_end;
        }
        let cursor_word_start = movement::move_prev_word_start(text, cursor_rev, 1);
        if cursor_word_start.anchor == next_grapheme_boundary(text, cursor) {
            cursor_rev = cursor_word_start;
        }
    }
    'outer: loop {
        let mut changed = false;
        while cursor_fwd.head < end {
            cursor_fwd = movement::move_next_word_end(text, cursor_fwd, 1);
            // The cursor is on a word that is atleast two graphemes long and
            // madeup of word characters. The latter condition is needed because
            // move_next_word_end simply treats a sequence of characters from
            // the same char class as a word so `=<` would also count as a word.
            let add_label = text
                .slice(..cursor_fwd.head)
                .graphemes_rev()
                .take(2)
                .take_while(|g| g.chars().all(char_is_word))
                .count()
                == 2;
            if !add_label {
                continue;
            }
            changed = true;
            // skip any leading whitespace
            cursor_fwd.anchor += text
                .chars_at(cursor_fwd.anchor)
                .take_while(|&c| !char_is_word(c))
                .count();
            words.push(cursor_fwd);
            if words.len() == jump_label_limit {
                break 'outer;
            }
            break;
        }
        while cursor_rev.head > start {
            cursor_rev = movement::move_prev_word_start(text, cursor_rev, 1);
            // The cursor is on a word that is atleast two graphemes long and
            // madeup of word characters. The latter condition is needed because
            // move_prev_word_start simply treats a sequence of characters from
            // the same char class as a word so `=<` would also count as a word.
            let add_label = text
                .slice(cursor_rev.head..)
                .graphemes()
                .take(2)
                .take_while(|g| g.chars().all(char_is_word))
                .count()
                == 2;
            if !add_label {
                continue;
            }
            changed = true;
            cursor_rev.anchor -= text
                .chars_at(cursor_rev.anchor)
                .reversed()
                .take_while(|&c| !char_is_word(c))
                .count();
            words.push(cursor_rev);
            if words.len() == jump_label_limit {
                break 'outer;
            }
            break;
        }
        if !changed {
            break;
        }
    }
    jump_to_label(cx, words, behaviour)
}

pub fn lsp_or_syntax_symbol_picker(cx: &mut Context) {
    let doc = doc!(cx.editor);

    if doc
        .language_servers_with_feature(LanguageServerFeature::DocumentSymbols)
        .next()
        .is_some()
    {
        lsp::symbol_picker(cx);
    } else if doc.syntax().is_some() {
        syntax_symbol_picker(cx);
    } else {
        cx.editor
            .set_error("No language server supporting document symbols or syntax info available");
    }
}

pub fn lsp_or_syntax_workspace_symbol_picker(cx: &mut Context) {
    let doc = doc!(cx.editor);

    if doc
        .language_servers_with_feature(LanguageServerFeature::WorkspaceSymbols)
        .next()
        .is_some()
    {
        lsp::workspace_symbol_picker(cx);
    } else {
        syntax_workspace_symbol_picker(cx);
    }
}
