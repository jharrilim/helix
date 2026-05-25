use helix_view::{
    agent::AgentModeMeta,
    info::Info,
    input::KeyEvent,
    keyboard::KeyCode,
    Editor,
};

use crate::agent;

pub fn open(editor: &mut Editor) {
    if editor.agent.available_modes.is_empty() {
        editor.set_error("no agent modes available");
        return;
    }

    editor.agent.mode_menu_active = true;
    refresh_info(editor);
}

pub fn close(editor: &mut Editor) {
    editor.agent.mode_menu_active = false;
    editor.autoinfo = None;
    helix_event::request_redraw();
}

pub fn handle_key(editor: &mut Editor, key: KeyEvent) -> bool {
    if !editor.agent.mode_menu_active {
        return false;
    }

    match key.code {
        KeyCode::Esc | KeyCode::Enter => {
            close(editor);
            true
        }
        KeyCode::Char(c @ '1'..='9') if key.modifiers.is_empty() => {
            let index = (c as u8 - b'1') as usize;
            select_mode(editor, index);
            true
        }
        _ => true,
    }
}

fn select_mode(editor: &mut Editor, index: usize) {
    let Some(mode) = editor.agent.available_modes.get(index).cloned() else {
        close(editor);
        return;
    };
    apply_mode(editor, &mode);
    close(editor);
}

fn apply_mode(editor: &mut Editor, mode: &AgentModeMeta) {
    agent::with_controller(|controller| {
        controller.send(helix_acp::AgentCommand::SetMode {
            mode_id: mode.id.clone(),
        });
    });
    editor.agent.mode = Some(mode.id.clone());
    editor.set_status(format!("agent mode: {}", mode.name));
}

fn refresh_info(editor: &mut Editor) {
    editor.autoinfo = Some(mode_menu_info(editor));
    helix_event::request_redraw();
}

fn mode_menu_info(editor: &Editor) -> Info {
    let mut body = Vec::new();

    for (index, mode) in editor.agent.available_modes.iter().enumerate().take(9) {
        let key = (index + 1).to_string();
        let desc = if editor.agent.mode.as_deref() == Some(mode.id.as_str()) {
            format!("{} (active)", mode.name)
        } else {
            mode.name.clone()
        };
        body.push((key, desc));
    }

    body.push(("esc".to_string(), "close".to_string()));

    Info::new("Agent mode", &body)
}
