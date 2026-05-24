use super::prelude::*;
use super::Context;

pub fn select_all(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);

    let end = doc.text().len_chars();
    doc.set_selection(view.id, Selection::single(0, end))
}

pub fn select_regex(cx: &mut Context) {
    let reg = cx.register.unwrap_or('/');
    ui::regex_prompt(
        cx,
        "select:".into(),
        Some(reg),
        ui::completers::none,
        move |cx, regex, event| {
            let (view, doc) = current!(cx.editor);
            if !matches!(event, PromptEvent::Update | PromptEvent::Validate) {
                return;
            }
            let text = doc.text().slice(..);
            if let Some(selection) =
                selection::select_on_matches(text, doc.selection(view.id), &regex)
            {
                doc.set_selection(view.id, selection);
            } else if event == PromptEvent::Validate {
                cx.editor.set_error("nothing selected");
            }
        },
    );
}

pub fn split_selection(cx: &mut Context) {
    let reg = cx.register.unwrap_or('/');
    ui::regex_prompt(
        cx,
        "split:".into(),
        Some(reg),
        ui::completers::none,
        move |cx, regex, event| {
            let (view, doc) = current!(cx.editor);
            if !matches!(event, PromptEvent::Update | PromptEvent::Validate) {
                return;
            }
            let text = doc.text().slice(..);
            let selection = selection::split_on_matches(text, doc.selection(view.id), &regex);
            doc.set_selection(view.id, selection);
        },
    );
}

pub fn split_selection_on_newline(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);
    let text = doc.text().slice(..);
    let selection = selection::split_on_newline(text, doc.selection(view.id));
    doc.set_selection(view.id, selection);
}

pub fn merge_selections(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);
    let selection = doc.selection(view.id).clone().merge_ranges();
    doc.set_selection(view.id, selection);
}

pub fn merge_consecutive_selections(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);
    let selection = doc.selection(view.id).clone().merge_consecutive_ranges();
    doc.set_selection(view.id, selection);
}

pub(crate) enum Extend {
    Above,
    Below,
}

pub fn extend_line(cx: &mut Context) {
    let (view, doc) = current_ref!(cx.editor);
    let extend = match doc.selection(view.id).primary().direction() {
        Direction::Forward => Extend::Below,
        Direction::Backward => Extend::Above,
    };
    extend_line_impl(cx, extend);
}

pub fn extend_line_below(cx: &mut Context) {
    extend_line_impl(cx, Extend::Below);
}

pub fn extend_line_above(cx: &mut Context) {
    extend_line_impl(cx, Extend::Above);
}
pub(crate) fn extend_line_impl(cx: &mut Context, extend: Extend) {
    let count = cx.count();
    let (view, doc) = current!(cx.editor);

    let text = doc.text();
    let selection = doc.selection(view.id).clone().transform(|range| {
        let (start_line, end_line) = range.line_range(text.slice(..));

        let start = text.line_to_char(start_line);
        let end = text.line_to_char(
            (end_line + 1) // newline of end_line
                .min(text.len_lines()),
        );

        // extend to previous/next line if current line is selected
        let (anchor, head) = if range.from() == start && range.to() == end {
            match extend {
                Extend::Above => (end, text.line_to_char(start_line.saturating_sub(count))),
                Extend::Below => (
                    start,
                    text.line_to_char((end_line + count + 1).min(text.len_lines())),
                ),
            }
        } else {
            match extend {
                Extend::Above => (end, text.line_to_char(start_line.saturating_sub(count - 1))),
                Extend::Below => (
                    start,
                    text.line_to_char((end_line + count).min(text.len_lines())),
                ),
            }
        };

        Range::new(anchor, head)
    });

    doc.set_selection(view.id, selection);
}
pub fn select_line_below(cx: &mut Context) {
    select_line_impl(cx, Extend::Below);
}
pub fn select_line_above(cx: &mut Context) {
    select_line_impl(cx, Extend::Above);
}
pub(crate) fn select_line_impl(cx: &mut Context, extend: Extend) {
    let mut count = cx.count();
    let (view, doc) = current!(cx.editor);
    let text = doc.text();
    let saturating_add = |a: usize, b: usize| (a + b).min(text.len_lines());
    let selection = doc.selection(view.id).clone().transform(|range| {
        let (start_line, end_line) = range.line_range(text.slice(..));
        let start = text.line_to_char(start_line);
        let end = text.line_to_char(saturating_add(end_line, 1));
        let direction = range.direction();

        // Extending to line bounds is counted as one step
        if range.from() != start || range.to() != end {
            count = count.saturating_sub(1)
        }
        let (anchor_line, head_line) = match (&extend, direction) {
            (Extend::Above, Direction::Forward) => (start_line, end_line.saturating_sub(count)),
            (Extend::Above, Direction::Backward) => (end_line, start_line.saturating_sub(count)),
            (Extend::Below, Direction::Forward) => (start_line, saturating_add(end_line, count)),
            (Extend::Below, Direction::Backward) => (end_line, saturating_add(start_line, count)),
        };
        let (anchor, head) = match anchor_line.cmp(&head_line) {
            Ordering::Less => (
                text.line_to_char(anchor_line),
                text.line_to_char(saturating_add(head_line, 1)),
            ),
            Ordering::Equal => match extend {
                Extend::Above => (
                    text.line_to_char(saturating_add(anchor_line, 1)),
                    text.line_to_char(head_line),
                ),
                Extend::Below => (
                    text.line_to_char(head_line),
                    text.line_to_char(saturating_add(anchor_line, 1)),
                ),
            },

            Ordering::Greater => (
                text.line_to_char(saturating_add(anchor_line, 1)),
                text.line_to_char(head_line),
            ),
        };
        Range::new(anchor, head)
    });

    doc.set_selection(view.id, selection);
}

pub fn extend_to_line_bounds(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);

    doc.set_selection(
        view.id,
        doc.selection(view.id).clone().transform(|range| {
            let text = doc.text();

            let (start_line, end_line) = range.line_range(text.slice(..));
            let start = text.line_to_char(start_line);
            let end = text.line_to_char((end_line + 1).min(text.len_lines()));

            Range::new(start, end).with_direction(range.direction())
        }),
    );
}

pub fn shrink_to_line_bounds(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);

    doc.set_selection(
        view.id,
        doc.selection(view.id).clone().transform(|range| {
            let text = doc.text();

            let (start_line, end_line) = range.line_range(text.slice(..));

            // Do nothing if the selection is within one line to prevent
            // conditional logic for the behavior of this command
            if start_line == end_line {
                return range;
            }

            let mut start = text.line_to_char(start_line);

            // line_to_char gives us the start position of the line, so
            // we need to get the start position of the next line. In
            // the editor, this will correspond to the cursor being on
            // the EOL whitespace character, which is what we want.
            let mut end = text.line_to_char((end_line + 1).min(text.len_lines()));

            if start != range.from() {
                start = text.line_to_char((start_line + 1).min(text.len_lines()));
            }

            if end != range.to() {
                end = text.line_to_char(end_line);
            }

            Range::new(start, end).with_direction(range.direction())
        }),
    );
}
pub fn rotate_selections(cx: &mut Context, direction: Direction) {
    let count = cx.count();
    let (view, doc) = current!(cx.editor);
    let mut selection = doc.selection(view.id).clone();
    let index = selection.primary_index();
    let len = selection.len();
    selection.set_primary_index(match direction {
        Direction::Forward => (index + count) % len,
        Direction::Backward => (index + (len.saturating_sub(count) % len)) % len,
    });
    doc.set_selection(view.id, selection);
}
pub fn rotate_selections_forward(cx: &mut Context) {
    rotate_selections(cx, Direction::Forward)
}
pub fn rotate_selections_backward(cx: &mut Context) {
    rotate_selections(cx, Direction::Backward)
}

pub fn rotate_selections_first(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);
    let mut selection = doc.selection(view.id).clone();
    selection.set_primary_index(0);
    doc.set_selection(view.id, selection);
}

pub fn rotate_selections_last(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);
    let mut selection = doc.selection(view.id).clone();
    let len = selection.len();
    selection.set_primary_index(len - 1);
    doc.set_selection(view.id, selection);
}

#[derive(Debug)]
pub(crate) enum ReorderStrategy {
    RotateForward,
    RotateBackward,
    Reverse,
}

pub(crate) fn reorder_selection_contents(cx: &mut Context, strategy: ReorderStrategy) {
    let count = cx.count;
    let (view, doc) = current!(cx.editor);
    let text = doc.text().slice(..);

    let selection = doc.selection(view.id);

    let mut ranges: Vec<_> = selection
        .slices(text)
        .map(|fragment| fragment.chunks().collect())
        .collect();

    let rotate_by = count.map_or(1, |count| count.get().min(ranges.len()));

    let primary_index = match strategy {
        ReorderStrategy::RotateForward => {
            ranges.rotate_right(rotate_by);
            // Like `usize::wrapping_add`, but provide a custom range from `0` to `ranges.len()`
            (selection.primary_index() + ranges.len() + rotate_by) % ranges.len()
        }
        ReorderStrategy::RotateBackward => {
            ranges.rotate_left(rotate_by);
            // Like `usize::wrapping_sub`, but provide a custom range from `0` to `ranges.len()`
            (selection.primary_index() + ranges.len() - rotate_by) % ranges.len()
        }
        ReorderStrategy::Reverse => {
            if rotate_by.is_multiple_of(2) {
                // nothing changed, if we reverse something an even
                // amount of times, the output will be the same
                return;
            }
            ranges.reverse();
            // -1 to turn 1-based len into 0-based index
            (ranges.len() - 1) - selection.primary_index()
        }
    };

    let transaction = Transaction::change(
        doc.text(),
        selection
            .ranges()
            .iter()
            .zip(ranges)
            .map(|(range, fragment)| (range.from(), range.to(), Some(fragment))),
    );

    doc.set_selection(
        view.id,
        Selection::new(selection.ranges().into(), primary_index),
    );
    doc.apply(&transaction, view.id);
}

pub fn rotate_selection_contents_forward(cx: &mut Context) {
    reorder_selection_contents(cx, ReorderStrategy::RotateForward)
}
pub fn rotate_selection_contents_backward(cx: &mut Context) {
    reorder_selection_contents(cx, ReorderStrategy::RotateBackward)
}
pub fn reverse_selection_contents(cx: &mut Context) {
    reorder_selection_contents(cx, ReorderStrategy::Reverse)
}

// tree sitter node selection

pub fn expand_selection(cx: &mut Context) {
    let motion = |editor: &mut Editor| {
        let (view, doc) = current!(editor);

        if let Some(syntax) = doc.syntax() {
            let text = doc.text().slice(..);

            let current_selection = doc.selection(view.id);
            let selection = object::expand_selection(syntax, text, current_selection.clone());

            // check if selection is different from the last one
            if *current_selection != selection {
                // save current selection so it can be restored using shrink_selection
                view.object_selections.push(current_selection.clone());

                doc.set_selection(view.id, selection);
            }
        }
    };
    cx.editor.apply_motion(motion);
}

pub fn shrink_selection(cx: &mut Context) {
    let motion = |editor: &mut Editor| {
        let (view, doc) = current!(editor);
        let current_selection = doc.selection(view.id);
        // try to restore previous selection
        if let Some(prev_selection) = view.object_selections.pop() {
            if current_selection.contains(&prev_selection) {
                doc.set_selection(view.id, prev_selection);
                return;
            } else {
                // clear existing selection as they can't be shrunk to anyway
                view.object_selections.clear();
            }
        }
        // if not previous selection, shrink to first child
        if let Some(syntax) = doc.syntax() {
            let text = doc.text().slice(..);
            let selection = object::shrink_selection(syntax, text, current_selection.clone());
            doc.set_selection(view.id, selection);
        }
    };
    cx.editor.apply_motion(motion);
}

pub fn select_sibling_impl<F>(cx: &mut Context, sibling_fn: F)
where
    F: Fn(&helix_core::Syntax, RopeSlice, Selection) -> Selection + 'static,
{
    let motion = move |editor: &mut Editor| {
        let (view, doc) = current!(editor);

        if let Some(syntax) = doc.syntax() {
            let text = doc.text().slice(..);
            let current_selection = doc.selection(view.id);
            let selection = sibling_fn(syntax, text, current_selection.clone());
            doc.set_selection(view.id, selection);
        }
    };
    cx.editor.apply_motion(motion);
}

pub fn select_next_sibling(cx: &mut Context) {
    select_sibling_impl(cx, object::select_next_sibling)
}

pub fn select_prev_sibling(cx: &mut Context) {
    select_sibling_impl(cx, object::select_prev_sibling)
}

pub fn move_node_bound_impl(cx: &mut Context, dir: Direction, movement: Movement) {
    let motion = move |editor: &mut Editor| {
        let (view, doc) = current!(editor);

        if let Some(syntax) = doc.syntax() {
            let text = doc.text().slice(..);
            let current_selection = doc.selection(view.id);

            let selection = movement::move_parent_node_end(
                syntax,
                text,
                current_selection.clone(),
                dir,
                movement,
            );

            doc.set_selection(view.id, selection);
        }
    };

    cx.editor.apply_motion(motion);
}

pub fn move_parent_node_end(cx: &mut Context) {
    move_node_bound_impl(cx, Direction::Forward, Movement::Move)
}

pub fn move_parent_node_start(cx: &mut Context) {
    move_node_bound_impl(cx, Direction::Backward, Movement::Move)
}

pub fn extend_parent_node_end(cx: &mut Context) {
    move_node_bound_impl(cx, Direction::Forward, Movement::Extend)
}

pub fn extend_parent_node_start(cx: &mut Context) {
    move_node_bound_impl(cx, Direction::Backward, Movement::Extend)
}

pub fn select_all_impl<F>(editor: &mut Editor, select_fn: F)
where
    F: Fn(&Syntax, RopeSlice, Selection) -> Selection,
{
    let (view, doc) = current!(editor);

    if let Some(syntax) = doc.syntax() {
        let text = doc.text().slice(..);
        let current_selection = doc.selection(view.id);
        let selection = select_fn(syntax, text, current_selection.clone());
        doc.set_selection(view.id, selection);
    }
}

pub fn select_all_siblings(cx: &mut Context) {
    let motion = |editor: &mut Editor| {
        select_all_impl(editor, object::select_all_siblings);
    };

    cx.editor.apply_motion(motion);
}

pub fn select_all_children(cx: &mut Context) {
    let motion = |editor: &mut Editor| {
        select_all_impl(editor, object::select_all_children);
    };

    cx.editor.apply_motion(motion);
}

