use helix_acp::AgentSessionId;
use helix_stdx::path;
use std::path::PathBuf;

use helix_view::{agent::AgentSessionMeta, Editor};

use super::Context;
use crate::agent;
use crate::compositor::Compositor;
use crate::job::{Callback, Jobs};
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
    close_agent_panel_editor(editor);
}

pub fn close_agent_panel_editor(editor: &mut Editor) {
    crate::ui::agent_cursor::cancel_pending_cursor_requests(editor);
    crate::ui::agent_permission::cancel_pending_permission(editor);
    agent::with_controller(|controller| controller.shutdown());
    editor.agent.active_session = None;
    editor.agent.mode = None;
    editor.agent.pending = false;
    editor.agent.pending_permission = None;
    editor.agent.cursor_request = None;
    editor.agent.cursor_question_flow = None;
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
    }
}

pub fn agent_new_editor(editor: &mut Editor, jobs: &mut Jobs) {
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

    editor.agent.clear_transcript();
    editor.agent.error = None;
    editor.agent.pending = true;
    editor.agent.status = Some("starting new agent session...".into());

    let cwd = editor
        .last_cwd
        .clone()
        .or_else(|| std::env::current_dir().ok());

    agent::with_controller(|controller| {
        controller.new_session(cwd);
    });
}

pub fn agent_clear_editor(editor: &mut Editor) {
    editor.agent.clear_transcript();
    editor.agent.error = None;
}

pub fn agent_history_editor(editor: &mut Editor, jobs: &mut Jobs, cwd_only: bool) {
    if !editor.agent_settings().enable {
        editor.set_error("agent integration is disabled in config");
        return;
    }

    if let Err(err) = agent::ensure_runtime_started(editor, jobs) {
        editor.set_error(format!("{err:#}"));
        return;
    }

    editor.agent.open_session_picker = true;
    editor.agent.history_filter_cwd = cwd_only;
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
    let preferred_cwd = editor
        .last_cwd
        .clone()
        .or_else(|| std::env::current_dir().ok());
    let mut sessions = editor.agent.sessions.clone();
    if editor.agent.history_filter_cwd {
        if let Some(cwd) = preferred_cwd.as_ref() {
            sessions.retain(|session| session.cwd == *cwd);
        }
        editor.agent.history_filter_cwd = false;
    }
    sort_agent_sessions(&mut sessions, preferred_cwd.as_deref());
    if sessions.is_empty() {
        editor.set_error("no agent sessions found");
        return;
    }

    let cwd_hint = preferred_cwd
        .as_ref()
        .map(|cwd| path::get_relative_path(cwd).to_string_lossy().into_owned());

    let columns = [
        PickerColumn::new("title", |item: &AgentSessionMeta, _| {
            item.title
                .as_deref()
                .filter(|title| !title.is_empty())
                .map(|title| title.to_string())
                .unwrap_or_else(|| {
                    path::get_relative_path(&item.cwd)
                        .to_string_lossy()
                        .into_owned()
                })
                .into()
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

    if let Some(hint) = cwd_hint.as_deref() {
        editor.set_status(format!("agent sessions (cwd: {hint})"));
    }

    compositor.push(Box::new(overlaid(picker)));
}

pub fn sort_agent_sessions(sessions: &mut [AgentSessionMeta], preferred_cwd: Option<&std::path::Path>) {
    sessions.sort_by(|left, right| {
        let left_preferred = preferred_cwd
            .map(|cwd| left.cwd == cwd)
            .unwrap_or(false);
        let right_preferred = preferred_cwd
            .map(|cwd| right.cwd == cwd)
            .unwrap_or(false);
        right_preferred
            .cmp(&left_preferred)
            .then_with(|| right.updated_at.cmp(&left.updated_at))
            .then_with(|| {
                left.title
                    .as_deref()
                    .unwrap_or("")
                    .cmp(right.title.as_deref().unwrap_or(""))
            })
            .then_with(|| right.id.cmp(&left.id))
    });
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
    agent_history_editor(cx.editor, cx.jobs, false);
}

pub fn agent_new(cx: &mut Context) {
    agent_new_editor(cx.editor, cx.jobs);
}

pub fn agent_mode_editor(editor: &mut Editor, mode_id: Option<String>) {
    if let Some(mode_id) = mode_id {
        agent::with_controller(|controller| {
            controller.send(helix_acp::AgentCommand::SetMode { mode_id: mode_id.clone() });
        });
        editor.agent.mode = Some(mode_id);
        editor.set_status("agent mode updated");
    }
}

pub fn open_mode_picker(cx: &mut Context) {
    if cx.editor.agent.available_modes.is_empty() {
        cx.editor.set_error("no agent modes available");
        return;
    }

    cx.callback.push(Box::new(|compositor, cx| {
        crate::ui::agent_cursor::show_mode_picker(cx.editor, compositor);
    }));
}

pub fn open_mode_picker_from_jobs(editor: &mut Editor, jobs: &mut Jobs) {
    if editor.agent.available_modes.is_empty() {
        editor.set_error("no agent modes available");
        return;
    }

    jobs.callback(async move {
        Ok(Callback::EditorCompositor(Box::new(|editor, compositor| {
            crate::ui::agent_cursor::show_mode_picker(editor, compositor);
        })))
    });
}

pub fn agent_mode(cx: &mut Context) {
    open_mode_picker(cx);
}
