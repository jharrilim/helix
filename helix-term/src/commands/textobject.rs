use super::prelude::*;
use super::{Context, helpers::*};

pub fn match_brackets(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);
    let is_select = cx.editor.mode == Mode::Select;
    let text = doc.text();
    let text_slice = text.slice(..);

    let selection = doc.selection(view.id).clone().transform(|range| {
        let pos = range.cursor(text_slice);
        if let Some(matched_pos) = doc.syntax().map_or_else(
            || match_brackets::find_matching_bracket_plaintext(text.slice(..), pos),
            |syntax| match_brackets::find_matching_bracket_fuzzy(syntax, text.slice(..), pos),
        ) {
            range.put_cursor(text_slice, matched_pos, is_select)
        } else {
            range
        }
    });

    doc.set_selection(view.id, selection);
}

//
pub fn goto_ts_object_impl(cx: &mut Context, object: &'static str, direction: Direction) {
    let count = cx.count();
    let motion = move |editor: &mut Editor| {
        let (view, doc) = current!(editor);
        let loader = editor.syn_loader.load();
        if let Some(syntax) = doc.syntax() {
            let text = doc.text().slice(..);
            let root = syntax.tree().root_node();

            let selection = doc.selection(view.id).clone().transform(|range| {
                let new_range = movement::goto_treesitter_object(
                    text, range, object, direction, &root, syntax, &loader, count,
                );

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
            doc.set_selection(view.id, selection);
        } else {
            editor.set_status("Syntax-tree is not available in current buffer");
        }
    };
    cx.editor.apply_motion(motion);
}

pub fn goto_next_function(cx: &mut Context) {
    goto_ts_object_impl(cx, "function", Direction::Forward)
}

pub fn goto_prev_function(cx: &mut Context) {
    goto_ts_object_impl(cx, "function", Direction::Backward)
}

pub fn goto_next_class(cx: &mut Context) {
    goto_ts_object_impl(cx, "class", Direction::Forward)
}

pub fn goto_prev_class(cx: &mut Context) {
    goto_ts_object_impl(cx, "class", Direction::Backward)
}

pub fn goto_next_parameter(cx: &mut Context) {
    goto_ts_object_impl(cx, "parameter", Direction::Forward)
}

pub fn goto_prev_parameter(cx: &mut Context) {
    goto_ts_object_impl(cx, "parameter", Direction::Backward)
}

pub fn goto_next_comment(cx: &mut Context) {
    goto_ts_object_impl(cx, "comment", Direction::Forward)
}

pub fn goto_prev_comment(cx: &mut Context) {
    goto_ts_object_impl(cx, "comment", Direction::Backward)
}

pub fn goto_next_test(cx: &mut Context) {
    goto_ts_object_impl(cx, "test", Direction::Forward)
}

pub fn goto_prev_test(cx: &mut Context) {
    goto_ts_object_impl(cx, "test", Direction::Backward)
}

pub fn goto_next_xml_element(cx: &mut Context) {
    goto_ts_object_impl(cx, "xml-element", Direction::Forward)
}

pub fn goto_prev_xml_element(cx: &mut Context) {
    goto_ts_object_impl(cx, "xml-element", Direction::Backward)
}

pub fn goto_next_entry(cx: &mut Context) {
    goto_ts_object_impl(cx, "entry", Direction::Forward)
}

pub fn goto_prev_entry(cx: &mut Context) {
    goto_ts_object_impl(cx, "entry", Direction::Backward)
}

pub fn select_textobject_around(cx: &mut Context) {
    select_textobject(cx, textobject::TextObject::Around);
}

pub fn select_textobject_inner(cx: &mut Context) {
    select_textobject(cx, textobject::TextObject::Inside);
}

pub fn select_textobject(cx: &mut Context, objtype: textobject::TextObject) {
    let count = cx.count();

    cx.on_next_key(move |cx, event| {
        cx.editor.autoinfo = None;
        if let Some(ch) = event.char() {
            let textobject = move |editor: &mut Editor| {
                let (view, doc) = current!(editor);
                let loader = editor.syn_loader.load();
                let text = doc.text().slice(..);

                let textobject_treesitter = |obj_name: &str, range: Range| -> Range {
                    let Some(syntax) = doc.syntax() else {
                        return range;
                    };
                    textobject::textobject_treesitter(
                        text, range, objtype, obj_name, syntax, &loader, count,
                    )
                };

                if ch == 'g' && doc.diff_handle().is_none() {
                    editor.set_status("Diff is not available in current buffer");
                    return;
                }

                let textobject_change = |range: Range| -> Range {
                    let diff_handle = doc.diff_handle().unwrap();
                    let diff = diff_handle.load();
                    let line = range.cursor_line(text);
                    let hunk_idx = if let Some(hunk_idx) = diff.hunk_at(line as u32, false) {
                        hunk_idx
                    } else {
                        return range;
                    };
                    let hunk = diff.nth_hunk(hunk_idx).after;

                    let start = text.line_to_char(hunk.start as usize);
                    let end = text.line_to_char(hunk.end as usize);
                    Range::new(start, end).with_direction(range.direction())
                };

                let selection = doc.selection(view.id).clone().transform(|range| {
                    match ch {
                        'w' => textobject::textobject_word(text, range, objtype, count, false),
                        'W' => textobject::textobject_word(text, range, objtype, count, true),
                        't' => textobject_treesitter("class", range),
                        'f' => textobject_treesitter("function", range),
                        'a' => textobject_treesitter("parameter", range),
                        'c' => textobject_treesitter("comment", range),
                        'T' => textobject_treesitter("test", range),
                        'e' => textobject_treesitter("entry", range),
                        'x' => textobject_treesitter("xml-element", range),
                        'p' => textobject::textobject_paragraph(text, range, objtype, count),
                        'm' => textobject::textobject_pair_surround_closest(
                            doc.syntax(),
                            text,
                            range,
                            objtype,
                            count,
                        ),
                        'g' => textobject_change(range),
                        // TODO: cancel new ranges if inconsistent surround matches across lines
                        ch if !ch.is_ascii_alphanumeric() => textobject::textobject_pair_surround(
                            doc.syntax(),
                            text,
                            range,
                            objtype,
                            ch,
                            count,
                        ),
                        _ => range,
                    }
                });
                doc.set_selection(view.id, selection);
            };
            cx.editor.apply_motion(textobject);
        }
    });

    let title = match objtype {
        textobject::TextObject::Inside => "Match inside",
        textobject::TextObject::Around => "Match around",
        _ => return,
    };
    let help_text = [
        ("w", "Word"),
        ("W", "WORD"),
        ("p", "Paragraph"),
        ("t", "Type definition (tree-sitter)"),
        ("f", "Function (tree-sitter)"),
        ("a", "Argument/parameter (tree-sitter)"),
        ("c", "Comment (tree-sitter)"),
        ("T", "Test (tree-sitter)"),
        ("e", "Data structure entry (tree-sitter)"),
        ("m", "Closest surrounding pair (tree-sitter)"),
        ("g", "Change"),
        ("x", "(X)HTML element (tree-sitter)"),
        (" ", "... or any character acting as a pair"),
    ];

    cx.editor.autoinfo = Some(Info::new(title, &help_text));
}

static SURROUND_HELP_TEXT: [(&str, &str); 6] = [
    ("m", "Nearest matching pair"),
    ("( or )", "Parentheses"),
    ("{ or }", "Curly braces"),
    ("< or >", "Angled brackets"),
    ("[ or ]", "Square brackets"),
    (" ", "... or any character"),
];

pub fn surround_add(cx: &mut Context) {
    cx.on_next_key(move |cx, event| {
        cx.editor.autoinfo = None;
        let (view, doc) = current!(cx.editor);
        // surround_len is the number of new characters being added.
        let (open, close, surround_len) = match event.char() {
            Some(ch) => {
                let (o, c) = match_brackets::get_pair(ch);
                let mut open = Tendril::new();
                open.push(o);
                let mut close = Tendril::new();
                close.push(c);
                (open, close, 2)
            }
            None if event.code == KeyCode::Enter => (
                doc.line_ending.as_str().into(),
                doc.line_ending.as_str().into(),
                2 * doc.line_ending.len_chars(),
            ),
            None => return,
        };

        let selection = doc.selection(view.id);
        let mut changes = Vec::with_capacity(selection.len() * 2);
        let mut ranges = SmallVec::with_capacity(selection.len());
        let mut offs = 0;

        for range in selection.iter() {
            changes.push((range.from(), range.from(), Some(open.clone())));
            changes.push((range.to(), range.to(), Some(close.clone())));

            ranges.push(
                Range::new(offs + range.from(), offs + range.to() + surround_len)
                    .with_direction(range.direction()),
            );

            offs += surround_len;
        }

        let transaction = Transaction::change(doc.text(), changes.into_iter())
            .with_selection(Selection::new(ranges, selection.primary_index()));
        doc.apply(&transaction, view.id);
        exit_select_mode(cx);
    });

    cx.editor.autoinfo = Some(Info::new(
        "Surround selections with",
        &SURROUND_HELP_TEXT[1..],
    ));
}

pub fn surround_replace(cx: &mut Context) {
    let count = cx.count();
    cx.on_next_key(move |cx, event| {
        cx.editor.autoinfo = None;
        let surround_ch = match event.char() {
            Some('m') => None, // m selects the closest surround pair
            Some(ch) => Some(ch),
            None => return,
        };
        let (view, doc) = current!(cx.editor);
        let text = doc.text().slice(..);
        let selection = doc.selection(view.id);

        let change_pos =
            match surround::get_surround_pos(doc.syntax(), text, selection, surround_ch, count) {
                Ok(c) => c,
                Err(err) => {
                    cx.editor.set_error(err.to_string());
                    return;
                }
            };

        let selection = selection.clone();
        let ranges: SmallVec<[Range; 1]> = change_pos.iter().map(|&p| Range::point(p)).collect();
        doc.set_selection(
            view.id,
            Selection::new(ranges, selection.primary_index() * 2),
        );

        cx.on_next_key(move |cx, event| {
            cx.editor.autoinfo = None;
            let (view, doc) = current!(cx.editor);
            let to = match event.char() {
                Some(to) => to,
                None => return doc.set_selection(view.id, selection),
            };
            let (open, close) = match_brackets::get_pair(to);

            // the changeset has to be sorted to allow nested surrounds
            let mut sorted_pos: Vec<(usize, char)> = Vec::new();
            for p in change_pos.chunks(2) {
                sorted_pos.push((p[0], open));
                sorted_pos.push((p[1], close));
            }
            sorted_pos.sort_unstable();

            let transaction = Transaction::change(
                doc.text(),
                sorted_pos.iter().map(|&pos| {
                    let mut t = Tendril::new();
                    t.push(pos.1);
                    (pos.0, pos.0 + 1, Some(t))
                }),
            );
            doc.set_selection(view.id, selection);
            doc.apply(&transaction, view.id);
            exit_select_mode(cx);
        });

        cx.editor.autoinfo = Some(Info::new(
            "Replace with a pair of",
            &SURROUND_HELP_TEXT[1..],
        ));
    });

    cx.editor.autoinfo = Some(Info::new(
        "Replace surrounding pair of",
        &SURROUND_HELP_TEXT,
    ));
}

pub fn surround_delete(cx: &mut Context) {
    let count = cx.count();
    cx.on_next_key(move |cx, event| {
        cx.editor.autoinfo = None;
        let surround_ch = match event.char() {
            Some('m') => None, // m selects the closest surround pair
            Some(ch) => Some(ch),
            None => return,
        };
        let (view, doc) = current!(cx.editor);
        let text = doc.text().slice(..);
        let selection = doc.selection(view.id);

        let mut change_pos =
            match surround::get_surround_pos(doc.syntax(), text, selection, surround_ch, count) {
                Ok(c) => c,
                Err(err) => {
                    cx.editor.set_error(err.to_string());
                    return;
                }
            };
        change_pos.sort_unstable(); // the changeset has to be sorted to allow nested surrounds
        let transaction =
            Transaction::change(doc.text(), change_pos.into_iter().map(|p| (p, p + 1, None)));
        doc.apply(&transaction, view.id);
        exit_select_mode(cx);
    });

    cx.editor.autoinfo = Some(Info::new("Delete surrounding pair of", &SURROUND_HELP_TEXT));
}
