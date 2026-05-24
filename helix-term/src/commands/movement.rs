use super::prelude::*;
use super::{Context, goto_line_impl, helpers::*};

pub fn no_op(_cx: &mut Context) {}

type MoveFn =
    fn(RopeSlice, Range, Direction, usize, Movement, &TextFormat, &mut TextAnnotations) -> Range;

pub fn move_impl(cx: &mut Context, move_fn: MoveFn, dir: Direction, behaviour: Movement) {
    let count = cx.count();
    let (view, doc) = current!(cx.editor);
    let text = doc.text().slice(..);
    let text_fmt = doc.text_format(view.inner_area(doc).width, None);
    let mut annotations = view.text_annotations(doc, None);

    let selection = doc.selection(view.id).clone().transform(|range| {
        move_fn(
            text,
            range,
            dir,
            count,
            behaviour,
            &text_fmt,
            &mut annotations,
        )
    });
    drop(annotations);
    doc.set_selection(view.id, selection);
}

use helix_core::movement::{move_horizontally, move_vertically};

pub fn move_char_left(cx: &mut Context) {
    move_impl(cx, move_horizontally, Direction::Backward, Movement::Move)
}

pub fn move_char_right(cx: &mut Context) {
    move_impl(cx, move_horizontally, Direction::Forward, Movement::Move)
}

pub fn move_line_up(cx: &mut Context) {
    move_impl(cx, move_vertically, Direction::Backward, Movement::Move)
}

pub fn move_line_down(cx: &mut Context) {
    move_impl(cx, move_vertically, Direction::Forward, Movement::Move)
}

pub fn move_visual_line_up(cx: &mut Context) {
    move_impl(
        cx,
        move_vertically_visual,
        Direction::Backward,
        Movement::Move,
    )
}

pub fn move_visual_line_down(cx: &mut Context) {
    move_impl(
        cx,
        move_vertically_visual,
        Direction::Forward,
        Movement::Move,
    )
}

pub fn extend_char_left(cx: &mut Context) {
    move_impl(cx, move_horizontally, Direction::Backward, Movement::Extend)
}

pub fn extend_char_right(cx: &mut Context) {
    move_impl(cx, move_horizontally, Direction::Forward, Movement::Extend)
}

pub fn extend_line_up(cx: &mut Context) {
    move_impl(cx, move_vertically, Direction::Backward, Movement::Extend)
}

pub fn extend_line_down(cx: &mut Context) {
    move_impl(cx, move_vertically, Direction::Forward, Movement::Extend)
}

pub fn extend_visual_line_up(cx: &mut Context) {
    move_impl(
        cx,
        move_vertically_visual,
        Direction::Backward,
        Movement::Extend,
    )
}

pub fn extend_visual_line_down(cx: &mut Context) {
    move_impl(
        cx,
        move_vertically_visual,
        Direction::Forward,
        Movement::Extend,
    )
}

pub fn goto_line_end_impl(view: &mut View, doc: &mut Document, movement: Movement) {
    let text = doc.text().slice(..);

    let selection = doc.selection(view.id).clone().transform(|range| {
        let line = range.cursor_line(text);
        let line_start = text.line_to_char(line);

        let pos = graphemes::prev_grapheme_boundary(text, line_end_char_index(&text, line))
            .max(line_start);

        range.put_cursor(text, pos, movement == Movement::Extend)
    });
    doc.set_selection(view.id, selection);
}

pub fn goto_line_end(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);
    goto_line_end_impl(
        view,
        doc,
        if cx.editor.mode == Mode::Select {
            Movement::Extend
        } else {
            Movement::Move
        },
    )
}

pub fn extend_to_line_end(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);
    goto_line_end_impl(view, doc, Movement::Extend)
}

pub fn goto_line_end_newline_impl(view: &mut View, doc: &mut Document, movement: Movement) {
    let text = doc.text().slice(..);

    let selection = doc.selection(view.id).clone().transform(|range| {
        let line = range.cursor_line(text);
        let pos = line_end_char_index(&text, line);

        range.put_cursor(text, pos, movement == Movement::Extend)
    });
    doc.set_selection(view.id, selection);
}

pub fn goto_line_end_newline(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);
    goto_line_end_newline_impl(
        view,
        doc,
        if cx.editor.mode == Mode::Select {
            Movement::Extend
        } else {
            Movement::Move
        },
    )
}

pub fn extend_to_line_end_newline(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);
    goto_line_end_newline_impl(view, doc, Movement::Extend)
}

pub fn goto_line_start_impl(view: &mut View, doc: &mut Document, movement: Movement) {
    let text = doc.text().slice(..);

    let selection = doc.selection(view.id).clone().transform(|range| {
        let line = range.cursor_line(text);

        // adjust to start of the line
        let pos = text.line_to_char(line);
        range.put_cursor(text, pos, movement == Movement::Extend)
    });
    doc.set_selection(view.id, selection);
}

pub fn goto_line_start(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);
    goto_line_start_impl(
        view,
        doc,
        if cx.editor.mode == Mode::Select {
            Movement::Extend
        } else {
            Movement::Move
        },
    )
}

pub fn goto_next_buffer(cx: &mut Context) {
    goto_buffer(cx.editor, Direction::Forward, cx.count());
}

pub fn goto_previous_buffer(cx: &mut Context) {
    goto_buffer(cx.editor, Direction::Backward, cx.count());
}

pub fn goto_buffer(editor: &mut Editor, direction: Direction, count: usize) {
    let current = view!(editor).doc;

    let id = match direction {
        Direction::Forward => {
            let iter = editor.documents.keys();
            // skip 'count' times past current buffer
            iter.cycle().skip_while(|id| *id != &current).nth(count)
        }
        Direction::Backward => {
            let iter = editor.documents.keys();
            // skip 'count' times past current buffer
            iter.rev()
                .cycle()
                .skip_while(|id| *id != &current)
                .nth(count)
        }
    }
    .unwrap();

    let id = *id;

    editor.switch(id, Action::Replace);
}

pub fn extend_to_line_start(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);
    goto_line_start_impl(view, doc, Movement::Extend)
}

pub fn goto_window(cx: &mut Context, align: Align) {
    let count = cx.count() - 1;
    let config = cx.editor.config();
    let (view, doc) = current!(cx.editor);
    let view_offset = doc.view_offset(view.id);

    let height = view.inner_height();

    // respect user given count if any
    // - 1 so we have at least one gap in the middle.
    // a height of 6 with padding of 3 on each side will keep shifting the view back and forth
    // as we type
    let scrolloff = config.scrolloff.min(height.saturating_sub(1) / 2);

    let last_visual_line = view.last_visual_line(doc);

    let visual_line = match align {
        Align::Top => view_offset.vertical_offset + scrolloff + count,
        Align::Center => view_offset.vertical_offset + (last_visual_line / 2),
        Align::Bottom => {
            view_offset.vertical_offset + last_visual_line.saturating_sub(scrolloff + count)
        }
    };
    let visual_line = visual_line
        .max(view_offset.vertical_offset + scrolloff)
        .min(view_offset.vertical_offset + last_visual_line.saturating_sub(scrolloff));

    let pos = view
        .pos_at_visual_coords(doc, visual_line as u16, 0, false)
        .expect("visual_line was constrained to the view area");

    let text = doc.text().slice(..);
    let selection = doc
        .selection(view.id)
        .clone()
        .transform(|range| range.put_cursor(text, pos, cx.editor.mode == Mode::Select));
    doc.set_selection(view.id, selection);
}

pub fn goto_window_top(cx: &mut Context) {
    goto_window(cx, Align::Top)
}

pub fn goto_window_center(cx: &mut Context) {
    goto_window(cx, Align::Center)
}

pub fn goto_window_bottom(cx: &mut Context) {
    goto_window(cx, Align::Bottom)
}

pub fn move_word_impl<F>(cx: &mut Context, move_fn: F)
where
    F: Fn(RopeSlice, Range, usize) -> Range,
{
    let count = cx.count();
    let (view, doc) = current!(cx.editor);
    let text = doc.text().slice(..);

    let selection = doc
        .selection(view.id)
        .clone()
        .transform(|range| move_fn(text, range, count));
    doc.set_selection(view.id, selection);
}

pub fn move_next_word_start(cx: &mut Context) {
    move_word_impl(cx, movement::move_next_word_start)
}

pub fn move_prev_word_start(cx: &mut Context) {
    move_word_impl(cx, movement::move_prev_word_start)
}

pub fn move_prev_word_end(cx: &mut Context) {
    move_word_impl(cx, movement::move_prev_word_end)
}

pub fn move_next_word_end(cx: &mut Context) {
    move_word_impl(cx, movement::move_next_word_end)
}

pub fn move_next_long_word_start(cx: &mut Context) {
    move_word_impl(cx, movement::move_next_long_word_start)
}

pub fn move_prev_long_word_start(cx: &mut Context) {
    move_word_impl(cx, movement::move_prev_long_word_start)
}

pub fn move_prev_long_word_end(cx: &mut Context) {
    move_word_impl(cx, movement::move_prev_long_word_end)
}

pub fn move_next_long_word_end(cx: &mut Context) {
    move_word_impl(cx, movement::move_next_long_word_end)
}

pub fn move_next_sub_word_start(cx: &mut Context) {
    move_word_impl(cx, movement::move_next_sub_word_start)
}

pub fn move_prev_sub_word_start(cx: &mut Context) {
    move_word_impl(cx, movement::move_prev_sub_word_start)
}

pub fn move_prev_sub_word_end(cx: &mut Context) {
    move_word_impl(cx, movement::move_prev_sub_word_end)
}

pub fn move_next_sub_word_end(cx: &mut Context) {
    move_word_impl(cx, movement::move_next_sub_word_end)
}

pub fn goto_para_impl<F>(cx: &mut Context, move_fn: F)
where
    F: Fn(RopeSlice, Range, usize, Movement) -> Range + 'static,
{
    let count = cx.count();
    let motion = move |editor: &mut Editor| {
        let (view, doc) = current!(editor);
        let text = doc.text().slice(..);
        let behavior = if editor.mode == Mode::Select {
            Movement::Extend
        } else {
            Movement::Move
        };

        let selection = doc
            .selection(view.id)
            .clone()
            .transform(|range| move_fn(text, range, count, behavior));
        doc.set_selection(view.id, selection);
    };
    cx.editor.apply_motion(motion)
}

pub fn goto_prev_paragraph(cx: &mut Context) {
    goto_para_impl(cx, movement::move_prev_paragraph)
}

pub fn goto_next_paragraph(cx: &mut Context) {
    goto_para_impl(cx, movement::move_next_paragraph)
}

pub fn goto_file_start(cx: &mut Context) {
    goto_file_start_impl(cx, Movement::Move);
}

pub fn extend_to_file_start(cx: &mut Context) {
    goto_file_start_impl(cx, Movement::Extend);
}

pub fn goto_file_start_impl(cx: &mut Context, movement: Movement) {
    if cx.count.is_some() {
        goto_line_impl(cx, movement);
    } else {
        let (view, doc) = current!(cx.editor);
        let text = doc.text().slice(..);
        let selection = doc
            .selection(view.id)
            .clone()
            .transform(|range| range.put_cursor(text, 0, movement == Movement::Extend));
        push_jump(view, doc);
        doc.set_selection(view.id, selection);
    }
}

pub fn goto_file_end(cx: &mut Context) {
    goto_file_end_impl(cx, Movement::Move);
}

pub fn extend_to_file_end(cx: &mut Context) {
    goto_file_end_impl(cx, Movement::Extend)
}

pub fn goto_file_end_impl(cx: &mut Context, movement: Movement) {
    let (view, doc) = current!(cx.editor);
    let text = doc.text().slice(..);
    let pos = doc.text().len_chars();
    let selection = doc
        .selection(view.id)
        .clone()
        .transform(|range| range.put_cursor(text, pos, movement == Movement::Extend));
    push_jump(view, doc);
    doc.set_selection(view.id, selection);
}

pub fn extend_word_impl<F>(cx: &mut Context, extend_fn: F)
where
    F: Fn(RopeSlice, Range, usize) -> Range,
{
    let count = cx.count();
    let (view, doc) = current!(cx.editor);
    let text = doc.text().slice(..);

    let selection = doc.selection(view.id).clone().transform(|range| {
        let word = extend_fn(text, range, count);
        let pos = word.cursor(text);
        range.put_cursor(text, pos, true)
    });
    doc.set_selection(view.id, selection);
}

pub fn extend_next_word_start(cx: &mut Context) {
    extend_word_impl(cx, movement::move_next_word_start)
}

pub fn extend_prev_word_start(cx: &mut Context) {
    extend_word_impl(cx, movement::move_prev_word_start)
}

pub fn extend_next_word_end(cx: &mut Context) {
    extend_word_impl(cx, movement::move_next_word_end)
}

pub fn extend_prev_word_end(cx: &mut Context) {
    extend_word_impl(cx, movement::move_prev_word_end)
}

pub fn extend_next_long_word_start(cx: &mut Context) {
    extend_word_impl(cx, movement::move_next_long_word_start)
}

pub fn extend_prev_long_word_start(cx: &mut Context) {
    extend_word_impl(cx, movement::move_prev_long_word_start)
}

pub fn extend_prev_long_word_end(cx: &mut Context) {
    extend_word_impl(cx, movement::move_prev_long_word_end)
}

pub fn extend_next_long_word_end(cx: &mut Context) {
    extend_word_impl(cx, movement::move_next_long_word_end)
}

pub fn extend_next_sub_word_start(cx: &mut Context) {
    extend_word_impl(cx, movement::move_next_sub_word_start)
}

pub fn extend_prev_sub_word_start(cx: &mut Context) {
    extend_word_impl(cx, movement::move_prev_sub_word_start)
}

pub fn extend_prev_sub_word_end(cx: &mut Context) {
    extend_word_impl(cx, movement::move_prev_sub_word_end)
}

pub fn extend_next_sub_word_end(cx: &mut Context) {
    extend_word_impl(cx, movement::move_next_sub_word_end)
}

/// Separate branch to find_char designed only for `<ret>` char.
//
// This is necessary because the one document can have different line endings inside. And we
// cannot predict what character to find when <ret> is pressed. On the current line it can be `lf`
// but on the next line it can be `crlf`. That's why [`find_char_impl`] cannot be applied here.
pub fn find_char_line_ending_motion(
    editor: &mut Editor,
    count: usize,
    direction: Direction,
    inclusive: bool,
    extend: bool,
) {
    let (view, doc) = current!(editor);
    let text = doc.text().slice(..);

    let selection = doc.selection(view.id).clone().transform(|range| {
        let cursor_anchor = range.cursor(text);
        let cursor_head = next_grapheme_boundary(text, cursor_anchor);
        let cursor_line = range.cursor_line(text);

        let pos = match direction {
            Direction::Forward => {
                let line_end = line_end_char_index(&text, cursor_line);
                let on_edge = if inclusive {
                    line_end == cursor_anchor
                } else {
                    line_end == cursor_head || line_end == cursor_anchor
                };
                let line = cursor_line + count - 1 + on_edge as usize;
                if line >= text.len_lines() - 1 {
                    return range;
                }
                line_end_char_index(&text, line) - !inclusive as usize
            }
            Direction::Backward => {
                if inclusive {
                    let line = cursor_line as isize - count as isize;
                    if line < 0 {
                        return range;
                    }
                    line_end_char_index(&text, line as usize)
                } else {
                    let on_edge = text.line_to_char(cursor_line) == cursor_anchor;
                    let line = cursor_line as isize - count as isize + 1 - on_edge as isize;
                    if line <= 0 {
                        return range;
                    }
                    text.line_to_char(line as usize)
                }
            }
        };

        if extend {
            range.put_cursor(text, pos, true)
        } else {
            Range::point(range.cursor(text)).put_cursor(text, pos, true)
        }
    });
    doc.set_selection(view.id, selection);
}

pub fn find_char(cx: &mut Context, direction: Direction, inclusive: bool, extend: bool) {
    // TODO: count is reset to 1 before next key so we move it into the closure here.
    // Would be nice to carry over.
    let count = cx.count();

    // need to wait for next key
    // TODO: should this be done by grapheme rather than char?  For example,
    // we can't properly handle the line-ending CRLF case here in terms of char.
    cx.on_next_key(move |cx, event| {
        let motion: Motion = if event.code == KeyCode::Enter {
            Box::new(move |editor: &mut Editor| {
                find_char_line_ending_motion(editor, count, direction, inclusive, extend);
            })
        } else if let Some(ch) = match event.code {
            KeyCode::Tab => Some('\t'),
            KeyCode::Char(ch) => Some(ch),
            _ => None,
        } {
            Box::new(move |editor: &mut Editor| {
                let (view, doc) = current!(editor);
                let text = doc.text().slice(..);

                let selection = doc.selection(view.id).clone().transform(|range| {
                    let cursor_anchor = range.cursor(text);
                    let cursor_head = next_grapheme_boundary(text, cursor_anchor);

                    // Exclusive search skips the next char after cursor to enable repeated application
                    let search_start_pos = match (inclusive, direction) {
                        (true, Direction::Forward) => cursor_head,
                        (true, Direction::Backward) => cursor_anchor,
                        (false, Direction::Forward) => cursor_head + 1,
                        (false, Direction::Backward) => cursor_anchor - 1,
                    };

                    search::find_nth_char(count, text, ch, search_start_pos, direction)
                        // Exclusive search should stop on previous character
                        .map(|pos| match (inclusive, direction) {
                            (true, Direction::Forward) => pos,
                            (true, Direction::Backward) => pos,
                            (false, Direction::Forward) => pos - 1,
                            (false, Direction::Backward) => pos + 1,
                        })
                        .map_or(range, |pos| {
                            if extend {
                                range.put_cursor(text, pos, true)
                            } else {
                                Range::point(range.cursor(text)).put_cursor(text, pos, true)
                            }
                        })
                });

                doc.set_selection(view.id, selection);
            })
        } else {
            return;
        };

        cx.editor.apply_motion(motion);
    })
}

pub fn find_till_char(cx: &mut Context) {
    find_char(cx, Direction::Forward, false, false);
}

pub fn find_next_char(cx: &mut Context) {
    find_char(cx, Direction::Forward, true, false)
}

pub fn extend_till_char(cx: &mut Context) {
    find_char(cx, Direction::Forward, false, true)
}

pub fn extend_next_char(cx: &mut Context) {
    find_char(cx, Direction::Forward, true, true)
}

pub fn till_prev_char(cx: &mut Context) {
    find_char(cx, Direction::Backward, false, false)
}

pub fn find_prev_char(cx: &mut Context) {
    find_char(cx, Direction::Backward, true, false)
}

pub fn extend_till_prev_char(cx: &mut Context) {
    find_char(cx, Direction::Backward, false, true)
}

pub fn extend_prev_char(cx: &mut Context) {
    find_char(cx, Direction::Backward, true, true)
}

pub fn repeat_last_motion(cx: &mut Context) {
    cx.editor.repeat_last_motion(cx.count())
}
pub fn scroll(cx: &mut Context, offset: usize, direction: Direction, sync_cursor: bool) {
    use Direction::*;
    let config = cx.editor.config();
    let (view, doc) = current!(cx.editor);
    let mut view_offset = doc.view_offset(view.id);

    let range = doc.selection(view.id).primary();
    let text = doc.text().slice(..);

    let cursor = range.cursor(text);
    let height = view.inner_height();

    let scrolloff = config.scrolloff.min(height.saturating_sub(1) / 2);
    let offset = match direction {
        Forward => offset as isize,
        Backward => -(offset as isize),
    };

    let doc_text = doc.text().slice(..);
    let viewport = view.inner_area(doc);
    let text_fmt = doc.text_format(viewport.width, None);
    (view_offset.anchor, view_offset.vertical_offset) = char_idx_at_visual_offset(
        doc_text,
        view_offset.anchor,
        view_offset.vertical_offset as isize + offset,
        0,
        &text_fmt,
        // &annotations,
        &view.text_annotations(&*doc, None),
    );
    doc.set_view_offset(view.id, view_offset);

    let doc_text = doc.text().slice(..);
    let mut annotations = view.text_annotations(&*doc, None);

    if sync_cursor {
        let movement = match cx.editor.mode {
            Mode::Select => Movement::Extend,
            _ => Movement::Move,
        };
        // TODO: When inline diagnostics gets merged- 1. move_vertically_visual removes
        // line annotations/diagnostics so the cursor may jump further than the view.
        // 2. If the cursor lands on a complete line of virtual text, the cursor will
        // jump a different distance than the view.
        let selection = doc.selection(view.id).clone().transform(|range| {
            move_vertically_visual(
                doc_text,
                range,
                direction,
                offset.unsigned_abs(),
                movement,
                &text_fmt,
                &mut annotations,
            )
        });
        drop(annotations);
        doc.set_selection(view.id, selection);
        return;
    }

    let view_offset = doc.view_offset(view.id);

    let mut head;
    match direction {
        Forward => {
            let off;
            (head, off) = char_idx_at_visual_offset(
                doc_text,
                view_offset.anchor,
                (view_offset.vertical_offset + scrolloff) as isize,
                0,
                &text_fmt,
                &annotations,
            );
            head += (off != 0) as usize;
            if head <= cursor {
                return;
            }
        }
        Backward => {
            head = char_idx_at_visual_offset(
                doc_text,
                view_offset.anchor,
                (view_offset.vertical_offset + height - scrolloff - 1) as isize,
                0,
                &text_fmt,
                &annotations,
            )
            .0;
            if head >= cursor {
                return;
            }
        }
    }

    let anchor = if cx.editor.mode == Mode::Select {
        range.anchor
    } else {
        head
    };

    // replace primary selection with an empty selection at cursor pos
    let prim_sel = Range::new(anchor, head);
    let mut sel = doc.selection(view.id).clone();
    let idx = sel.primary_index();
    sel = sel.replace(idx, prim_sel);
    drop(annotations);
    doc.set_selection(view.id, sel);
}

pub fn page_up(cx: &mut Context) {
    let view = view!(cx.editor);
    let offset = view.inner_height();
    scroll(cx, offset, Direction::Backward, false);
}

pub fn page_down(cx: &mut Context) {
    let view = view!(cx.editor);
    let offset = view.inner_height();
    scroll(cx, offset, Direction::Forward, false);
}

pub fn half_page_up(cx: &mut Context) {
    let view = view!(cx.editor);
    let offset = view.inner_height() / 2;
    scroll(cx, offset, Direction::Backward, false);
}

pub fn half_page_down(cx: &mut Context) {
    let view = view!(cx.editor);
    let offset = view.inner_height() / 2;
    scroll(cx, offset, Direction::Forward, false);
}

pub fn page_cursor_up(cx: &mut Context) {
    let view = view!(cx.editor);
    let offset = view.inner_height();
    scroll(cx, offset, Direction::Backward, true);
}

pub fn page_cursor_down(cx: &mut Context) {
    let view = view!(cx.editor);
    let offset = view.inner_height();
    scroll(cx, offset, Direction::Forward, true);
}

pub fn page_cursor_half_up(cx: &mut Context) {
    let view = view!(cx.editor);
    let offset = view.inner_height() / 2;
    scroll(cx, offset, Direction::Backward, true);
}

pub fn page_cursor_half_down(cx: &mut Context) {
    let view = view!(cx.editor);
    let offset = view.inner_height() / 2;
    scroll(cx, offset, Direction::Forward, true);
}

#[allow(deprecated)]
// currently uses the deprecated `visual_coords_at_pos`/`pos_at_visual_coords` functions
// as this function ignores softwrapping (and virtual text) and instead only cares
// about "text visual position"
//
// TODO: implement a variant of that uses visual lines and respects virtual text
pub fn copy_selection_on_line(cx: &mut Context, direction: Direction) {
    use helix_core::{pos_at_visual_coords, visual_coords_at_pos};

    let count = cx.count();
    let (view, doc) = current!(cx.editor);
    let text = doc.text().slice(..);
    let selection = doc.selection(view.id);
    let mut ranges = SmallVec::with_capacity(selection.ranges().len() * (count + 1));
    ranges.extend_from_slice(selection.ranges());
    let mut primary_index = 0;
    for range in selection.iter() {
        let is_primary = *range == selection.primary();

        // The range is always head exclusive
        let (head, anchor) = if range.anchor < range.head {
            (range.head - 1, range.anchor)
        } else {
            (range.head, range.anchor.saturating_sub(1))
        };

        let tab_width = doc.tab_width();

        let head_pos = visual_coords_at_pos(text, head, tab_width);
        let anchor_pos = visual_coords_at_pos(text, anchor, tab_width);

        let height = std::cmp::max(head_pos.row, anchor_pos.row)
            - std::cmp::min(head_pos.row, anchor_pos.row)
            + 1;

        if is_primary {
            primary_index = ranges.len();
        }
        ranges.push(*range);

        let mut sels = 0;
        let mut i = 0;
        while sels < count {
            let offset = (i + 1) * height;

            let anchor_row = match direction {
                Direction::Forward => anchor_pos.row + offset,
                Direction::Backward => anchor_pos.row.saturating_sub(offset),
            };

            let head_row = match direction {
                Direction::Forward => head_pos.row + offset,
                Direction::Backward => head_pos.row.saturating_sub(offset),
            };

            if anchor_row >= text.len_lines() || head_row >= text.len_lines() {
                break;
            }

            let anchor =
                pos_at_visual_coords(text, Position::new(anchor_row, anchor_pos.col), tab_width);
            let head = pos_at_visual_coords(text, Position::new(head_row, head_pos.col), tab_width);

            // skip lines that are too short
            if visual_coords_at_pos(text, anchor, tab_width).col == anchor_pos.col
                && visual_coords_at_pos(text, head, tab_width).col == head_pos.col
            {
                if is_primary {
                    primary_index = ranges.len();
                }
                // This is Range::new(anchor, head), but it will place the cursor on the correct column
                ranges.push(Range::point(anchor).put_cursor(text, head, true));
                sels += 1;
            }

            if anchor_row == 0 && head_row == 0 {
                break;
            }

            i += 1;
        }
    }

    let selection = Selection::new(ranges, primary_index);
    doc.set_selection(view.id, selection);
}

pub fn copy_selection_on_prev_line(cx: &mut Context) {
    copy_selection_on_line(cx, Direction::Backward)
}

pub fn copy_selection_on_next_line(cx: &mut Context) {
    copy_selection_on_line(cx, Direction::Forward)
}
