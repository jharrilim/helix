use std::path::PathBuf;

use helix_pty::TerminalCommand;
use helix_view::{
    terminal::{TerminalCwd, TerminalFocus},
    Editor,
};

use super::Context;
use crate::terminal;
use crate::ui::{self, Picker, PickerColumn};
use crate::ui::overlay::overlaid;

pub fn spawn_terminal_session(
    _editor: &mut Editor,
    session_id: String,
    cwd: PathBuf,
    rows: u16,
    cols: u16,
) {
    terminal::send(TerminalCommand::Spawn {
        id: session_id.clone().into(),
        cwd: Some(cwd),
        rows,
        cols,
    });
}

pub fn terminal_open_editor(
    editor: &mut Editor,
    jobs: &mut crate::job::Jobs,
    cwd_override: Option<PathBuf>,
) {
    if !editor.integrated_terminal_settings().enable {
        editor.set_error("integrated terminal is disabled in config");
        return;
    }

    if let Err(err) = terminal::ensure_runtime_started(editor, jobs) {
        editor.set_error(format!("{err:#}"));
        return;
    }

    let session_id = editor.terminal.next_session_id();
    let cwd = cwd_override.unwrap_or_else(|| resolve_terminal_cwd(editor));
    editor.terminal.insert_session(session_id.clone(), cwd.clone());
    editor.open_terminal_panel(session_id.clone());

    let (rows, cols) = terminal_grid_size(editor);

    spawn_terminal_session(editor, session_id.clone(), cwd, rows, cols);

    if editor.integrated_terminal_settings().focus_on_open {
        editor.terminal.focus = TerminalFocus::Normal;
    }

    editor.set_status(format!("opened terminal {session_id}"));
}

pub fn terminal_new_editor(editor: &mut Editor, jobs: &mut crate::job::Jobs) {
    if !editor.integrated_terminal_settings().enable {
        editor.set_error("integrated terminal is disabled in config");
        return;
    }

    if let Err(err) = terminal::ensure_runtime_started(editor, jobs) {
        editor.set_error(format!("{err:#}"));
        return;
    }

    if !editor.terminal.is_open() {
        terminal_open_editor(editor, jobs, None);
        return;
    }

    let session_id = editor.terminal.next_session_id();
    let cwd = resolve_terminal_cwd(editor);
    editor.terminal.insert_session(session_id.clone(), cwd.clone());
    editor.open_terminal_panel(session_id.clone());

    let (rows, cols) = terminal_grid_size(editor);
    spawn_terminal_session(editor, session_id.clone(), cwd, rows, cols);
    editor.set_status(format!("opened terminal {session_id}"));
}

pub fn terminal_focus_editor(editor: &mut Editor, session_id: &str) {
    if !editor.terminal.sessions.contains_key(session_id) {
        editor.set_error(format!("unknown terminal session {session_id}"));
        return;
    }
    if !editor.terminal.is_open() {
        editor.set_error("terminal panel is not open");
        return;
    }
    editor.switch_terminal_session(session_id);
    editor.set_status(format!("focused terminal {session_id}"));
}

struct TerminalListItem {
    id: String,
    label: String,
}

pub fn show_terminal_list_picker(editor: &mut Editor, compositor: &mut crate::compositor::Compositor) {
    if editor.terminal.session_order.is_empty() {
        editor.set_error("no terminal sessions");
        return;
    }

    let sessions: Vec<TerminalListItem> = editor
        .terminal
        .ordered_sessions()
        .map(|session| TerminalListItem {
            id: session.id.clone(),
            label: session.tab_label().to_string(),
        })
        .collect();

    let columns = [
        PickerColumn::new("session", |item: &TerminalListItem, _| {
            item.label.as_str().into()
        }),
        PickerColumn::new("id", |item: &TerminalListItem, _| item.id.as_str().into()),
    ];

    let picker = Picker::new(columns, 0, sessions, (), move |cx, session, _action| {
        cx.editor.switch_terminal_session(&session.id);
        cx.editor.set_status(format!("focused terminal {}", session.id));
    });

    compositor.push(Box::new(overlaid(picker)));
}

pub fn terminal_list(cx: &mut Context) {
    if cx.editor.terminal.session_order.is_empty() {
        cx.editor.set_error("no terminal sessions");
        return;
    }
    cx.callback.push(Box::new(|compositor, cx| {
        show_terminal_list_picker(cx.editor, compositor);
    }));
}

pub fn terminal_focus(cx: &mut Context, session_id: &str) {
    terminal_focus_editor(cx.editor, session_id);
}

pub fn terminal_close_editor(editor: &mut Editor) {
    let Some(session_id) = editor.terminal.active_session.clone() else {
        editor.set_error("no active terminal session");
        return;
    };
    close_terminal_session(editor, &session_id);
}

pub fn close_terminal_session(editor: &mut Editor, session_id: &str) {
    terminal::send(TerminalCommand::Kill {
        id: session_id.to_string().into(),
    });
    terminal::with_controller(|controller| {
        controller.remove_session(session_id);
    });
    editor.remove_terminal_tab(session_id);
    editor.set_status(format!("closed terminal {session_id}"));
}

pub fn close_terminal_panel_editor(editor: &mut Editor) {
    if !editor.terminal.is_open() {
        return;
    }

    let session_ids: Vec<String> = editor.terminal.session_order.clone();
    for id in session_ids {
        terminal::send(TerminalCommand::Kill {
            id: id.clone().into(),
        });
        terminal::with_controller(|controller| {
            controller.remove_session(&id);
        });
    }

    editor.terminal.sessions.clear();
    editor.terminal.session_order.clear();
    editor.terminal.active_session = None;
    editor.terminal.clear_selection();
    editor.terminal.clear_search();
    editor.terminal.open_search_prompt = false;
    editor.terminal.tab_menu_active = false;
    editor.autoinfo = None;
    editor.terminal.register_pending = false;
    editor.terminal.pending_scroll_top = false;
    editor.terminal.focus = TerminalFocus::Normal;
    editor.close_terminal_panel();
}

pub fn terminal_toggle_editor(editor: &mut Editor, jobs: &mut crate::job::Jobs) {
    if !editor.integrated_terminal_settings().enable {
        editor.set_error("integrated terminal is disabled in config");
        return;
    }

    if editor.terminal.is_open() {
        terminal_close_editor(editor);
    } else {
        terminal_open_editor(editor, jobs, None);
    }
}

pub fn terminal_insert_mode(cx: &mut Context) {
    if !cx.editor.tree.is_terminal_panel(cx.editor.tree.focus) {
        cx.editor.set_error("terminal panel is not focused");
        return;
    }
    cx.editor.terminal.focus = TerminalFocus::Insert;
    cx.editor.mode = helix_view::document::Mode::Insert;
}

pub fn terminal_send_editor(editor: &mut Editor, append_newline: bool) {
    if !editor.terminal.is_open() {
        editor.set_error("no open terminal session");
        return;
    }

    let Some(text) = editor_send_text(editor) else {
        editor.set_error("no text to send");
        return;
    };

    let payload = if append_newline {
        format!("{text}\n")
    } else {
        text
    };

    let Some(session_id) = editor.terminal.active_session.clone() else {
        editor.set_error("no active terminal session");
        return;
    };

    terminal::send(TerminalCommand::Write {
        id: session_id.into(),
        data: payload.into_bytes(),
    });
    editor.set_status("sent text to terminal");
}

fn terminal_grid_size(editor: &Editor) -> (u16, u16) {
    if let Some(panel_id) = editor.terminal.panel_id {
        if let Some(panel) = editor.tree.terminal_panel(panel_id) {
            return ui::terminal::grid_size(panel.area);
        }
    }
    (24, 80)
}

fn editor_send_text(editor: &Editor) -> Option<String> {
    for (view, focus) in editor.tree.views() {
        if !focus {
            continue;
        }
        let doc = editor.documents.get(&view.doc)?;
        let selection = doc.selection(view.id).primary();
        let text = doc.text();
        if selection.from() != selection.to() {
            return Some(text.slice(selection.from()..selection.to()).to_string());
        }
        let line = text.char_to_line(selection.from());
        let start = text.line_to_char(line);
        let end = text.line_to_char(line + 1);
        return Some(text.slice(start..end).to_string().trim_end().to_string());
    }
    None
}

fn focused_buffer_dir(editor: &Editor) -> Option<PathBuf> {
    for (view, focus) in editor.tree.views() {
        if focus {
            let doc = editor.documents.get(&view.doc)?;
            return doc.path()?.parent().map(|path| path.to_path_buf());
        }
    }
    None
}

fn resolve_terminal_cwd(editor: &Editor) -> PathBuf {
    match editor.integrated_terminal_settings().cwd {
        TerminalCwd::Current => focused_buffer_dir(editor)
            .or_else(|| editor.last_cwd.clone())
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from(".")),
        TerminalCwd::WorkspaceRoot => {
            let (root, _) = helix_loader::find_workspace();
            if root.as_os_str().is_empty() {
                focused_buffer_dir(editor)
                    .or_else(|| editor.last_cwd.clone())
                    .or_else(|| std::env::current_dir().ok())
                    .unwrap_or_else(|| PathBuf::from("."))
            } else {
                root
            }
        }
    }
}

pub fn terminal_open(cx: &mut Context) {
    terminal_open_editor(cx.editor, cx.jobs, None);
}

pub fn terminal_new(cx: &mut Context) {
    terminal_new_editor(cx.editor, cx.jobs);
}

pub fn terminal_close(cx: &mut Context) {
    terminal_close_editor(cx.editor);
}

pub fn terminal_toggle(cx: &mut Context) {
    terminal_toggle_editor(cx.editor, cx.jobs);
}

pub fn terminal_send(cx: &mut Context) {
    terminal_send_editor(cx.editor, true);
}
