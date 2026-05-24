use super::prelude::*;
use super::{paste, scroll, Paste, Context, helpers::*, typed};

pub fn jump_forward(cx: &mut Context) {
    cx.editor.jump_forward(cx.editor.tree.focus, cx.count());
}

pub fn jump_backward(cx: &mut Context) {
    cx.editor.jump_backward(cx.editor.tree.focus, cx.count());
}

pub fn save_selection(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);
    push_jump(view, doc);
    cx.editor.set_status("Selection saved to jumplist");
}

pub fn rotate_view(cx: &mut Context) {
    cx.editor.focus_next()
}

pub fn rotate_view_reverse(cx: &mut Context) {
    cx.editor.focus_prev()
}

pub fn jump_view_right(cx: &mut Context) {
    cx.editor.focus_direction(tree::Direction::Right)
}

pub fn jump_view_left(cx: &mut Context) {
    cx.editor.focus_direction(tree::Direction::Left)
}

pub fn jump_view_up(cx: &mut Context) {
    cx.editor.focus_direction(tree::Direction::Up)
}

pub fn jump_view_down(cx: &mut Context) {
    cx.editor.focus_direction(tree::Direction::Down)
}

pub fn swap_view_right(cx: &mut Context) {
    cx.editor.swap_split_in_direction(tree::Direction::Right)
}

pub fn swap_view_left(cx: &mut Context) {
    cx.editor.swap_split_in_direction(tree::Direction::Left)
}

pub fn swap_view_up(cx: &mut Context) {
    cx.editor.swap_split_in_direction(tree::Direction::Up)
}

pub fn swap_view_down(cx: &mut Context) {
    cx.editor.swap_split_in_direction(tree::Direction::Down)
}

pub fn transpose_view(cx: &mut Context) {
    cx.editor.transpose_view()
}

/// Open a new split in the given direction specified by the action.
///
/// Maintain the current view (both the cursor's position and view in document).
pub fn split(editor: &mut Editor, action: Action) {
    if editor.tree.is_agent_panel(editor.tree.focus) {
        editor.set_error("cannot split from the agent panel");
        return;
    }

    let (view, doc) = current!(editor);
    let id = doc.id();
    let selection = doc.selection(view.id).clone();
    let offset = doc.view_offset(view.id);

    editor.switch(id, action);

    // match the selection in the previous view
    let (view, doc) = current!(editor);
    doc.set_selection(view.id, selection);
    // match the view scroll offset (switch doesn't handle this fully
    // since the selection is only matched after the split)
    doc.set_view_offset(view.id, offset);
}

pub fn hsplit(cx: &mut Context) {
    split(cx.editor, Action::HorizontalSplit);
}

pub fn hsplit_new(cx: &mut Context) {
    cx.editor.new_file(Action::HorizontalSplit);
}

pub fn vsplit(cx: &mut Context) {
    split(cx.editor, Action::VerticalSplit);
}

pub fn vsplit_new(cx: &mut Context) {
    cx.editor.new_file(Action::VerticalSplit);
}

pub fn wclose(cx: &mut Context) {
    if cx.editor.tree.is_agent_panel(cx.editor.tree.focus) {
        crate::commands::agent::close_agent_panel_editor(cx.editor);
        return;
    }

    if cx.editor.tree.is_terminal_panel(cx.editor.tree.focus) {
        if let Some(session_id) = cx.editor.terminal.active_session.clone() {
            crate::commands::terminal::close_terminal_session(cx.editor, &session_id);
        }
        return;
    }

    if cx.editor.tree.views().count() == 1 {
        if let Err(err) = typed::buffers_remaining_impl(cx.editor) {
            cx.editor.set_error(err.to_string());
            return;
        }
    }
    let view_id = view!(cx.editor).id;
    // close current split
    cx.editor.close(view_id);
}

pub fn wonly(cx: &mut Context) {
    let views = cx
        .editor
        .tree
        .views()
        .map(|(v, focus)| (v.id, focus))
        .collect::<Vec<_>>();
    for (view_id, focus) in views {
        if !focus {
            cx.editor.close(view_id);
        }
    }
}

pub fn select_register(cx: &mut Context) {
    cx.editor.autoinfo = Some(Info::from_registers(
        "Select register",
        &cx.editor.registers,
    ));
    cx.on_next_key(move |cx, event| {
        cx.editor.autoinfo = None;
        if let Some(ch) = event.char() {
            cx.editor.selected_register = Some(ch);
        }
    })
}

pub fn insert_register(cx: &mut Context) {
    cx.editor.autoinfo = Some(Info::from_registers(
        "Insert register",
        &cx.editor.registers,
    ));
    cx.on_next_key(move |cx, event| {
        cx.editor.autoinfo = None;
        if let Some(ch) = event.char() {
            cx.register = Some(ch);
            paste(
                cx.editor,
                cx.register
                    .unwrap_or(cx.editor.config().default_yank_register),
                Paste::Cursor,
                cx.count(),
            );
        }
    })
}

pub fn copy_between_registers(cx: &mut Context) {
    cx.editor.autoinfo = Some(Info::from_registers(
        "Copy from register",
        &cx.editor.registers,
    ));
    cx.on_next_key(move |cx, event| {
        cx.editor.autoinfo = None;

        let Some(source) = event.char() else {
            return;
        };

        let Some(values) = cx.editor.registers.read(source, cx.editor) else {
            cx.editor.set_error(format!("register {source} is empty"));
            return;
        };
        let values: Vec<_> = values.map(|value| value.to_string()).collect();

        cx.editor.autoinfo = Some(Info::from_registers(
            "Copy into register",
            &cx.editor.registers,
        ));
        cx.on_next_key(move |cx, event| {
            cx.editor.autoinfo = None;

            let Some(dest) = event.char() else {
                return;
            };

            let n_values = values.len();
            match cx.editor.registers.write(dest, values) {
                Ok(_) => cx.editor.set_status(format!(
                    "yanked {n_values} value{} from register {source} to {dest}",
                    if n_values == 1 { "" } else { "s" }
                )),
                Err(err) => cx.editor.set_error(err.to_string()),
            }
        });
    });
}

pub fn align_view_top(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);
    align_view(doc, view, Align::Top);
}

pub fn align_view_center(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);
    align_view(doc, view, Align::Center);
}

pub fn align_view_bottom(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);
    align_view(doc, view, Align::Bottom);
}

pub fn align_view_middle(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);
    let inner_width = view.inner_width(doc);
    let text_fmt = doc.text_format(inner_width, None);
    // there is no horizontal position when softwrap is enabled
    if text_fmt.soft_wrap {
        return;
    }
    let doc_text = doc.text().slice(..);
    let pos = doc.selection(view.id).primary().cursor(doc_text);
    let pos = visual_offset_from_block(
        doc_text,
        doc.view_offset(view.id).anchor,
        pos,
        &text_fmt,
        &view.text_annotations(doc, None),
    )
    .0;

    let mut offset = doc.view_offset(view.id);
    offset.horizontal_offset = pos
        .col
        .saturating_sub((view.inner_area(doc).width as usize) / 2);
    doc.set_view_offset(view.id, offset);
}

pub fn scroll_up(cx: &mut Context) {
    scroll(cx, cx.count(), Direction::Backward, false);
}

pub fn scroll_down(cx: &mut Context) {
    scroll(cx, cx.count(), Direction::Forward, false);
}
