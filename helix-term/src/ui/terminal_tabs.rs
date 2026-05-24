use crate::job::Jobs;
use helix_view::{
    info::Info,
    input::{KeyEvent, KeyModifiers},
    keyboard::KeyCode,
    Editor,
};

pub fn open(editor: &mut Editor) {
    editor.terminal.tab_menu_active = true;
    refresh_info(editor);
}

pub fn close(editor: &mut Editor) {
    editor.terminal.tab_menu_active = false;
    editor.autoinfo = None;
    helix_event::request_redraw();
}

pub fn handle_key(editor: &mut Editor, jobs: &mut Jobs, key: KeyEvent) -> bool {
    if !editor.terminal.tab_menu_active {
        return false;
    }

    match key.code {
        KeyCode::Esc | KeyCode::Enter => {
            close(editor);
            true
        }
        KeyCode::Char('h') | KeyCode::Left | KeyCode::Char('[') => {
            switch_relative(editor, -1);
            true
        }
        KeyCode::Char('l') | KeyCode::Right | KeyCode::Char(']') => {
            switch_relative(editor, 1);
            true
        }
        KeyCode::Char(c @ '1'..='9') if key.modifiers.is_empty() => {
            let index = (c as u8 - b'1') as usize;
            switch_to_index(editor, index);
            true
        }
        KeyCode::Char('n') if key.modifiers.is_empty() => {
            open_new(editor, jobs);
            true
        }
        KeyCode::Char('q') if key.modifiers.is_empty() => {
            close_active(editor);
            true
        }
        KeyCode::Char('t') if !key.modifiers.contains(KeyModifiers::SHIFT) => {
            close(editor);
            true
        }
        _ => true,
    }
}

fn open_new(editor: &mut Editor, jobs: &mut Jobs) {
    crate::commands::terminal::terminal_new_editor(editor, jobs);
    close(editor);
}

fn close_active(editor: &mut Editor) {
    let Some(session_id) = editor.terminal.active_session.clone() else {
        close(editor);
        return;
    };
    crate::commands::terminal::close_terminal_session(editor, &session_id);
    close(editor);
}

fn switch_relative(editor: &mut Editor, delta: isize) {
    if editor.terminal.switch_relative(delta) {
        if let Some(id) = editor.terminal.active_session.clone() {
            editor.switch_terminal_session(&id);
        }
    }
    close(editor);
}

fn switch_to_index(editor: &mut Editor, index: usize) {
    if let Some(id) = editor.terminal.session_order.get(index).cloned() {
        if editor.terminal.switch_session(&id) {
            editor.switch_terminal_session(&id);
        }
    }
    close(editor);
}

fn refresh_info(editor: &mut Editor) {
    editor.autoinfo = Some(tab_menu_info(editor));
    helix_event::request_redraw();
}

fn tab_menu_info(editor: &Editor) -> Info {
    let mut body = Vec::new();

    if !editor.terminal.session_order.is_empty() {
        for (index, id) in editor.terminal.session_order.iter().enumerate() {
            if index >= 9 {
                break;
            }
            let label = editor
                .terminal
                .sessions
                .get(id)
                .map(|session| session.tab_label())
                .unwrap_or(id.as_str());
            let key = (index + 1).to_string();
            let desc = if editor.terminal.active_session.as_deref() == Some(id.as_str()) {
                format!("{label} (active)")
            } else {
                label.to_string()
            };
            body.push((key, desc));
        }

        body.push(("h, [".to_string(), "previous tab".to_string()));
        body.push(("l, ]".to_string(), "next tab".to_string()));
    }

    body.push(("n".to_string(), "new terminal".to_string()));
    if !editor.terminal.session_order.is_empty() {
        body.push(("q".to_string(), "close active terminal".to_string()));
    }
    body.push(("esc".to_string(), "close".to_string()));

    Info::new("Terminal tabs", &body)
}
