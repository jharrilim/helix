use helix_acp::AgentSessionId;
use helix_stdx::path;
use std::path::PathBuf;

use helix_view::{agent::AgentSessionMeta, Editor};

use super::Context;
use crate::agent;
use crate::compositor::Compositor;
use crate::job::Jobs;
use crate::ui::{overlay::overlaid, Picker, PickerColumn};

pub fn agent_open_editor(editor: &mut Editor, jobs: &mut Jobs) {
    if !editor.agent_settings().enable {
        editor.set_error("agent integration is disabled in config");
        return;
    }

    editor.open_agent_panel();
    if let Err(err) = agent::ensure_runtime(editor, jobs) {
        editor.set_error(format!("{err:#}"));
    }
}

pub fn agent_close_editor(editor: &mut Editor) {
    editor.close_agent_panel();
}

pub fn agent_focus_editor_panel(editor: &mut Editor, jobs: &mut Jobs) {
    if !editor.agent.is_open() {
        agent_open_editor(editor, jobs);
        return;
    }
    editor.focus_agent_panel();
}

pub fn agent_focus_editor_from_panel(editor: &mut Editor) {
    editor.focus_editor_from_agent();
}

pub fn agent_send_editor(editor: &mut Editor, jobs: &mut Jobs) {
    if !editor.agent.is_open() {
        agent_open_editor(editor, jobs);
    }
    let text = editor.agent.input.trim().to_string();
    if text.is_empty() {
        editor.set_error("agent prompt is empty");
        return;
    }
    if let Err(err) = agent::ensure_runtime(editor, jobs) {
        editor.set_error(format!("{err:#}"));
        return;
    }
    agent::with_controller(|controller| {
        crate::handlers::agent::send_prompt(controller, editor, text);
    });
}

pub fn agent_stop_editor(editor: &mut Editor) {
    let was_pending = editor.agent.pending;
    agent::with_controller(|controller| {
        if was_pending {
            controller.cancel_turn();
        } else {
            controller.end_session();
        }
    });
    editor.agent.pending = false;
    if was_pending {
        editor.agent.status = Some("agent turn cancelled".into());
    } else {
        editor.agent.active_session = None;
        editor.agent.mode = None;
        editor.agent.status = Some("agent session stopped".into());
    }
}

pub fn agent_clear_editor(editor: &mut Editor) {
    editor.agent.clear_transcript();
    editor.agent.error = None;
}

pub fn agent_history_editor(editor: &mut Editor, jobs: &mut Jobs) {
    if !editor.agent_settings().enable {
        editor.set_error("agent integration is disabled in config");
        return;
    }

    if let Err(err) = agent::ensure_runtime_started(editor, jobs) {
        editor.set_error(format!("{err:#}"));
        return;
    }

    editor.agent.open_session_picker = true;
    editor.agent.pending = true;
    editor.set_status("loading agent sessions...");

    let cwd = editor
        .last_cwd
        .clone()
        .or_else(|| std::env::current_dir().ok());

    agent::with_controller(|controller| {
        controller.send(helix_acp::AgentCommand::ListSessions { cwd });
    });
}

pub fn show_history_picker(editor: &mut Editor, compositor: &mut Compositor) {
    let sessions = editor.agent.sessions.clone();
    if sessions.is_empty() {
        editor.set_error("no agent sessions found");
        return;
    }

    let columns = [
        PickerColumn::new("title", |item: &AgentSessionMeta, _| {
            item.title.as_deref().unwrap_or(&item.id).into()
        }),
        PickerColumn::new("updated", |item: &AgentSessionMeta, _| {
            item.updated_at.as_deref().unwrap_or("").into()
        }),
        PickerColumn::new("cwd", |item: &AgentSessionMeta, _| {
            path::get_relative_path(&item.cwd)
                .to_string_lossy()
                .into_owned()
                .into()
        }),
        PickerColumn::new("id", |item: &AgentSessionMeta, _| item.id.as_str().into()),
    ];

    let picker = Picker::new(columns, 0, sessions, (), move |cx, session, _action| {
        load_session_editor(
            cx.editor,
            cx.jobs,
            session.id.clone(),
            Some(session.cwd.clone()),
        );
    })
    .truncate_start(false);

    compositor.push(Box::new(overlaid(picker)));
}

pub fn load_session_editor(
    editor: &mut Editor,
    jobs: &mut Jobs,
    session_id: String,
    cwd: Option<PathBuf>,
) {
    if !editor.agent_settings().enable {
        editor.set_error("agent integration is disabled in config");
        return;
    }

    editor.open_agent_panel();
    editor.focus_agent_panel();

    if let Err(err) = agent::ensure_runtime_started(editor, jobs) {
        editor.set_error(format!("{err:#}"));
        return;
    }

    editor.agent.push_debug(format!("ui: picker load session {session_id}"));
    if let Some(ref cwd) = cwd {
        editor.agent.push_debug(format!("ui: picker cwd {}", cwd.display()));
    }

    editor.agent.pending = true;
    editor.agent.status = Some("loading agent session...".into());

    agent::with_controller(|controller| {
        controller.send(helix_acp::AgentCommand::LoadSession {
            session_id: AgentSessionId(session_id),
            cwd,
        });
    });
}

pub fn agent_open(cx: &mut Context) {
    agent_open_editor(cx.editor, cx.jobs);
}

pub fn agent_close(cx: &mut Context) {
    agent_close_editor(cx.editor);
}

pub fn agent_focus(cx: &mut Context) {
    agent_focus_editor_panel(cx.editor, cx.jobs);
}

pub fn agent_focus_editor(cx: &mut Context) {
    agent_focus_editor_from_panel(cx.editor);
}

pub fn agent_send(cx: &mut Context) {
    agent_send_editor(cx.editor, cx.jobs);
}

pub fn agent_stop(cx: &mut Context) {
    agent_stop_editor(cx.editor);
}

pub fn agent_clear(cx: &mut Context) {
    agent_clear_editor(cx.editor);
}

pub fn agent_history(cx: &mut Context) {
    agent_history_editor(cx.editor, cx.jobs);
}

pub fn agent_mode_editor(editor: &mut Editor, mode_id: Option<String>) {
    if let Some(mode_id) = mode_id {
        agent::with_controller(|controller| {
            controller.send(helix_acp::AgentCommand::SetMode { mode_id: mode_id.clone() });
        });
        editor.agent.mode = Some(mode_id);
        editor.set_status("agent mode updated");
        return;
    }

    if editor.agent.available_modes.is_empty() {
        editor.set_error("no agent modes available");
        return;
    }

    editor.agent.open_mode_picker = true;
}

pub fn agent_mode(cx: &mut Context) {
    agent_mode_editor(cx.editor, None);
}
