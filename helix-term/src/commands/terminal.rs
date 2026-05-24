use std::path::PathBuf;

use helix_pty::TerminalCommand;
use helix_view::{
    terminal::{TerminalCwd, TerminalFocus},
    Editor,
};

use super::Context;
use crate::terminal;
use crate::ui;

pub fn terminal_open_editor(editor: &mut Editor, jobs: &mut crate::job::Jobs) {
    if !editor.integrated_terminal_settings().enable {
        editor.set_error("integrated terminal is disabled in config");
        return;
    }

    if let Err(err) = terminal::ensure_runtime_started(editor, jobs) {
        editor.set_error(format!("{err:#}"));
        return;
    }

    let session_id = editor.terminal.next_session_id();
    let cwd = resolve_terminal_cwd(editor);
    editor.terminal.register_session(session_id.clone(), cwd);
    editor.open_terminal_panel(session_id.clone());

    let panel_id = editor
        .terminal
        .session_mut(&session_id)
        .and_then(|session| session.panel_id);
    let Some(panel_id) = panel_id else {
        editor.set_error("failed to open terminal panel");
        return;
    };

    let panel = editor.tree.terminal_panel(panel_id).unwrap();
    let (rows, cols) = ui::terminal::grid_size(panel.area);

    terminal::send(TerminalCommand::Spawn {
        id: session_id.clone().into(),
        cwd: editor
            .terminal
            .sessions
            .get(&session_id)
            .map(|session| session.cwd.clone()),
        rows,
        cols,
    });

    if editor.integrated_terminal_settings().focus_on_open {
        editor.terminal.focus = TerminalFocus::Normal;
    }

    editor.set_status(format!("opened terminal {session_id}"));
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
    editor.close_terminal_panel(session_id);
    editor.terminal.remove_session(session_id);
    editor.set_status(format!("closed terminal {session_id}"));
}

pub fn terminal_toggle_editor(editor: &mut Editor, jobs: &mut crate::job::Jobs) {
    if !editor.integrated_terminal_settings().enable {
        editor.set_error("integrated terminal is disabled in config");
        return;
    }

    let active_open = editor
        .terminal
        .active_session
        .as_ref()
        .and_then(|id| editor.terminal.sessions.get(id))
        .map(|session| session.panel_id.is_some())
        .unwrap_or(false);

    if active_open {
        terminal_close_editor(editor);
    } else {
        terminal_open_editor(editor, jobs);
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

fn resolve_terminal_cwd(editor: &Editor) -> PathBuf {
    match editor.integrated_terminal_settings().cwd {
        TerminalCwd::Current => editor
            .last_cwd
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from(".")),
        TerminalCwd::WorkspaceRoot => {
            let (root, _) = helix_loader::find_workspace();
            if root.as_os_str().is_empty() {
                editor
                    .last_cwd
                    .clone()
                    .or_else(|| std::env::current_dir().ok())
                    .unwrap_or_else(|| PathBuf::from("."))
            } else {
                root
            }
        }
    }
}

pub fn terminal_open(cx: &mut Context) {
    terminal_open_editor(cx.editor, cx.jobs);
}

pub fn terminal_close(cx: &mut Context) {
    terminal_close_editor(cx.editor);
}

pub fn terminal_toggle(cx: &mut Context) {
    terminal_toggle_editor(cx.editor, cx.jobs);
}
