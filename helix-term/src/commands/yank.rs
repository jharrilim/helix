use super::prelude::*;
use super::{Context, helpers::*};

pub fn yank(cx: &mut Context) {
    yank_impl(
        cx.editor,
        cx.register
            .unwrap_or(cx.editor.config().default_yank_register),
    );
    exit_select_mode(cx);
}

pub fn yank_to_clipboard(cx: &mut Context) {
    yank_impl(cx.editor, '+');
    exit_select_mode(cx);
}

pub fn yank_to_primary_clipboard(cx: &mut Context) {
    yank_impl(cx.editor, '*');
    exit_select_mode(cx);
}

pub fn yank_impl(editor: &mut Editor, register: char) {
    let (view, doc) = current!(editor);
    let text = doc.text().slice(..);

    let values: Vec<String> = doc
        .selection(view.id)
        .fragments(text)
        .map(Cow::into_owned)
        .collect();
    let selections = values.len();

    match editor.registers.write(register, values) {
        Ok(_) => editor.set_status(format!(
            "yanked {selections} selection{} to register {register}",
            if selections == 1 { "" } else { "s" }
        )),
        Err(err) => editor.set_error(err.to_string()),
    }
}

pub fn yank_joined_impl(editor: &mut Editor, separator: &str, register: char) {
    let (view, doc) = current!(editor);
    let text = doc.text().slice(..);

    let selection = doc.selection(view.id);
    let selections = selection.len();
    let joined = selection
        .fragments(text)
        .fold(String::new(), |mut acc, fragment| {
            if !acc.is_empty() {
                acc.push_str(separator);
            }
            acc.push_str(&fragment);
            acc
        });

    match editor.registers.write(register, vec![joined]) {
        Ok(_) => editor.set_status(format!(
            "joined and yanked {selections} selection{} to register {register}",
            if selections == 1 { "" } else { "s" }
        )),
        Err(err) => editor.set_error(err.to_string()),
    }
}

pub fn yank_joined(cx: &mut Context) {
    let separator = doc!(cx.editor).line_ending.as_str();
    yank_joined_impl(
        cx.editor,
        separator,
        cx.register
            .unwrap_or(cx.editor.config().default_yank_register),
    );
    exit_select_mode(cx);
}

pub fn yank_joined_to_clipboard(cx: &mut Context) {
    let line_ending = doc!(cx.editor).line_ending;
    yank_joined_impl(cx.editor, line_ending.as_str(), '+');
    exit_select_mode(cx);
}

pub fn yank_joined_to_primary_clipboard(cx: &mut Context) {
    let line_ending = doc!(cx.editor).line_ending;
    yank_joined_impl(cx.editor, line_ending.as_str(), '*');
    exit_select_mode(cx);
}

pub(crate) fn yank_main_selection_to_register(editor: &mut Editor, register: char) {
    let (view, doc) = current!(editor);
    let text = doc.text().slice(..);

    let selection = doc.selection(view.id).primary().fragment(text).to_string();

    match editor.registers.write(register, vec![selection]) {
        Ok(_) => editor.set_status(format!("yanked primary selection to register {register}",)),
        Err(err) => editor.set_error(err.to_string()),
    }
}

pub fn yank_main_selection_to_clipboard(cx: &mut Context) {
    yank_main_selection_to_register(cx.editor, '+');
    exit_select_mode(cx);
}

pub fn yank_main_selection_to_primary_clipboard(cx: &mut Context) {
    yank_main_selection_to_register(cx.editor, '*');
    exit_select_mode(cx);
}

#[derive(Copy, Clone)]
pub(crate) enum Paste {
    Before,
    After,
    Cursor,
}

static LINE_ENDING_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"\r\n|\r|\n").unwrap());

pub fn paste_impl(
    values: &[String],
    doc: &mut Document,
    view: &mut View,
    action: Paste,
    count: usize,
    mode: Mode,
) {
    if values.is_empty() {
        return;
    }

    if mode == Mode::Insert {
        doc.append_changes_to_history(view);
    }

    // if any of values ends with a line ending, it's linewise paste
    let linewise = values
        .iter()
        .any(|value| get_line_ending_of_str(value).is_some());

    let map_value = |value| {
        let value = LINE_ENDING_REGEX.replace_all(value, doc.line_ending.as_str());
        let mut out = Tendril::from(value.as_ref());
        for _ in 1..count {
            out.push_str(&value);
        }
        out
    };

    let repeat = std::iter::repeat(
        // `values` is asserted to have at least one entry above.
        map_value(values.last().unwrap()),
    );

    let mut values = values.iter().map(|value| map_value(value)).chain(repeat);

    let text = doc.text();
    let selection = doc.selection(view.id);

    let mut offset = 0;
    let mut ranges = SmallVec::with_capacity(selection.len());

    let mut transaction = Transaction::change_by_selection(text, selection, |range| {
        let pos = match (action, linewise) {
            // paste linewise before
            (Paste::Before, true) => text.line_to_char(text.char_to_line(range.from())),
            // paste linewise after
            (Paste::After, true) => {
                let line = range.line_range(text.slice(..)).1;
                text.line_to_char((line + 1).min(text.len_lines()))
            }
            // paste insert
            (Paste::Before, false) => range.from(),
            // paste append
            (Paste::After, false) => range.to(),
            // paste at cursor
            (Paste::Cursor, _) => range.cursor(text.slice(..)),
        };

        let value = values.next();

        let value_len = value
            .as_ref()
            .map(|content| content.chars().count())
            .unwrap_or_default();
        let anchor = offset + pos;

        let new_range = Range::new(anchor, anchor + value_len).with_direction(range.direction());
        ranges.push(new_range);
        offset += value_len;

        (pos, pos, value)
    });

    if mode == Mode::Normal {
        transaction = transaction.with_selection(Selection::new(ranges, selection.primary_index()));
    }

    doc.apply(&transaction, view.id);
    doc.append_changes_to_history(view);
}

pub fn paste_bracketed_value(cx: &mut Context, contents: String) {
    let count = cx.count();
    let paste = match cx.editor.mode {
        Mode::Insert | Mode::Select => Paste::Cursor,
        Mode::Normal => Paste::Before,
    };
    let (view, doc) = current!(cx.editor);
    paste_impl(&[contents], doc, view, paste, count, cx.editor.mode);
    exit_select_mode(cx);
}

pub fn paste_clipboard_after(cx: &mut Context) {
    paste(cx.editor, '+', Paste::After, cx.count());
    exit_select_mode(cx);
}

pub fn paste_clipboard_before(cx: &mut Context) {
    paste(cx.editor, '+', Paste::Before, cx.count());
    exit_select_mode(cx);
}

pub fn paste_primary_clipboard_after(cx: &mut Context) {
    paste(cx.editor, '*', Paste::After, cx.count());
    exit_select_mode(cx);
}

pub fn paste_primary_clipboard_before(cx: &mut Context) {
    paste(cx.editor, '*', Paste::Before, cx.count());
    exit_select_mode(cx);
}

pub fn replace_with_yanked(cx: &mut Context) {
    replace_selections_with_register(
        cx.editor,
        cx.register
            .unwrap_or(cx.editor.config().default_yank_register),
        cx.count(),
    );
    exit_select_mode(cx);
}

pub(crate) fn replace_selections_with_register(editor: &mut Editor, register: char, count: usize) {
    let Some(values) = editor
        .registers
        .read(register, editor)
        .filter(|values| values.len() > 0)
    else {
        return;
    };
    let scrolloff = editor.config().scrolloff;
    let (view, doc) = current_ref!(editor);

    let map_value = |value: &Cow<str>| {
        let value = LINE_ENDING_REGEX.replace_all(value, doc.line_ending.as_str());
        let mut out = Tendril::from(value.as_ref());
        for _ in 1..count {
            out.push_str(&value);
        }
        out
    };
    let mut values_rev = values.rev().peekable();
    // `values` is asserted to have at least one entry above.
    let last = values_rev.peek().unwrap();
    let repeat = std::iter::repeat(map_value(last));
    let mut values = values_rev
        .rev()
        .map(|value| map_value(&value))
        .chain(repeat);
    let selection = doc.selection(view.id);
    let transaction = Transaction::change_by_selection(doc.text(), selection, |range| {
        if !range.is_empty() {
            (range.from(), range.to(), Some(values.next().unwrap()))
        } else {
            (range.from(), range.to(), None)
        }
    });
    drop(values);

    let (view, doc) = current!(editor);
    doc.apply(&transaction, view.id);
    doc.append_changes_to_history(view);
    view.ensure_cursor_in_view(doc, scrolloff);
}

pub fn replace_selections_with_clipboard(cx: &mut Context) {
    replace_selections_with_register(cx.editor, '+', cx.count());
    exit_select_mode(cx);
}

pub fn replace_selections_with_primary_clipboard(cx: &mut Context) {
    replace_selections_with_register(cx.editor, '*', cx.count());
    exit_select_mode(cx);
}

pub fn paste(editor: &mut Editor, register: char, pos: Paste, count: usize) {
    let Some(values) = editor.registers.read(register, editor) else {
        return;
    };
    let values: Vec<_> = values.map(|value| value.to_string()).collect();

    let (view, doc) = current!(editor);
    paste_impl(&values, doc, view, pos, count, editor.mode);
}

pub fn paste_after(cx: &mut Context) {
    paste(
        cx.editor,
        cx.register
            .unwrap_or(cx.editor.config().default_yank_register),
        Paste::After,
        cx.count(),
    );
    exit_select_mode(cx);
}

pub fn paste_before(cx: &mut Context) {
    paste(
        cx.editor,
        cx.register
            .unwrap_or(cx.editor.config().default_yank_register),
        Paste::Before,
        cx.count(),
    );
    exit_select_mode(cx);
}
