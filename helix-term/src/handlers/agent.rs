//! ACP agent runtime integration and buffer-aware filesystem handlers.

use std::path::PathBuf;
use std::sync::mpsc::{self, SyncSender};

use helix_acp::{
    AgentCommand, AgentConfig, AgentEvent, AgentMessage, AgentRuntime, AgentRuntimeHandle,
    FsReadFn, FsReadResult, FsWriteFn, FsWriteRequest, FsWriteResult,
};
use helix_core::{diff, Rope};
use helix_view::{
    agent::{AgentCursorRequest, AgentModeMeta, AgentPermissionOption, AgentPendingPermission, AgentSessionMeta, AgentTranscriptEntry},
    Editor, View, ViewId,
};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::job;

/// Bridges ACP filesystem callbacks to the main thread.
struct FsBridge {
    read_requests: mpsc::Receiver<(PathBuf, SyncSender<FsReadResult>)>,
    write_requests: mpsc::Receiver<(FsWriteRequest, SyncSender<FsWriteResult>)>,
}

impl FsBridge {
    fn new() -> (Self, FsReadFn, FsWriteFn) {
        let (read_tx, read_rx) = mpsc::channel();
        let (write_tx, write_rx) = mpsc::channel();

        let read_fn: FsReadFn = Arc::new({
            let read_tx = read_tx.clone();
            move |path| {
                let (resp_tx, resp_rx) = mpsc::sync_channel(1);
                if read_tx.send((path, resp_tx)).is_err() {
                    return FsReadResult {
                        content: String::new(),
                    };
                }
                helix_event::request_redraw();
                resp_rx.recv().unwrap_or(FsReadResult {
                    content: String::new(),
                })
            }
        });

        let write_fn: FsWriteFn = Arc::new({
            let write_tx = write_tx.clone();
            move |request| {
                let (resp_tx, resp_rx) = mpsc::sync_channel(1);
                if write_tx.send((request, resp_tx)).is_err() {
                    return FsWriteResult::Error("agent filesystem bridge closed".into());
                }
                helix_event::request_redraw();
                resp_rx.recv().unwrap_or(FsWriteResult::Error(
                    "agent filesystem bridge closed".into(),
                ))
            }
        });

        (
            Self {
                read_requests: read_rx,
                write_requests: write_rx,
            },
            read_fn,
            write_fn,
        )
    }

    fn poll(&mut self, editor: &mut Editor) {
        while let Ok((path, resp)) = self.read_requests.try_recv() {
            let _ = resp.send(fs_read_impl(editor, path));
        }
        while let Ok((request, resp)) = self.write_requests.try_recv() {
            let _ = resp.send(fs_write_impl(editor, request));
        }
    }
}

use std::sync::Arc;

pub struct AgentController {
    runtime: Option<AgentRuntimeHandle>,
    events_rx: Option<UnboundedReceiver<AgentEvent>>,
    fs_bridge: Option<FsBridge>,
}

impl Default for AgentController {
    fn default() -> Self {
        Self {
            runtime: None,
            events_rx: None,
            fs_bridge: None,
        }
    }
}

impl AgentController {
    pub fn is_running(&self) -> bool {
        self.runtime.is_some()
    }

    pub fn start(&mut self, editor: &Editor) -> anyhow::Result<()> {
        let settings = editor.agent_settings();
        if settings.command.is_empty() {
            anyhow::bail!("agent command is not configured (see `[editor.agent]` in config)");
        }

        let (fs_bridge, fs_read, fs_write) = FsBridge::new();
        let config = AgentConfig {
            command: settings.command.clone(),
            client_name: "helix".into(),
            client_version: env!("CARGO_PKG_VERSION").into(),
            auth_method: settings.auth_method.clone(),
            skip_authenticate: settings.skip_authenticate,
            default_mode: settings.default_mode.clone(),
            mcp_config_path: settings.mcp_config_path.clone(),
            debug_logging: settings.debug_logging,
            auto_approve_permissions: settings.auto_approve_permissions,
        };

        let (handle, events_rx) = AgentRuntime::spawn(config, fs_read, fs_write);
        self.runtime = Some(handle);
        self.events_rx = Some(events_rx);
        self.fs_bridge = Some(fs_bridge);
        Ok(())
    }

    pub fn end_session(&self) {
        if let Some(handle) = &self.runtime {
            handle.send(AgentCommand::CloseSession);
        }
    }

    pub fn new_session(&self, cwd: Option<PathBuf>) {
        if let Some(handle) = &self.runtime {
            handle.send(AgentCommand::NewSession { cwd });
        }
    }

    pub fn cancel_turn(&self) {
        if let Some(handle) = &self.runtime {
            handle.send(AgentCommand::Cancel);
        }
    }

    pub fn shutdown(&mut self) {
        if let Some(handle) = self.runtime.take() {
            handle.send(AgentCommand::Stop);
        }
        self.events_rx = None;
        self.fs_bridge = None;
    }

    pub fn send(&self, cmd: AgentCommand) {
        if let Some(handle) = &self.runtime {
            handle.send(cmd);
        }
    }

    pub fn poll(&mut self, editor: &mut Editor) {
        if let Some(bridge) = self.fs_bridge.as_mut() {
            bridge.poll(editor);
        }
    }

    pub fn spawn_event_listener(&mut self, jobs: &mut crate::job::Jobs) {
        let Some(mut rx) = self.events_rx.take() else {
            return;
        };

        jobs.spawn(async move {
            while let Some(event) = rx.recv().await {
                job::dispatch(move |editor, compositor| {
                    apply_event(editor, &event);
                    if editor.agent.open_session_picker {
                        editor.agent.open_session_picker = false;
                        crate::commands::agent::show_history_picker(editor, compositor);
                    }
                    if editor.agent.open_cursor_request {
                        editor.agent.open_cursor_request = false;
                        if editor.agent.cursor_question_flow.is_some() {
                            crate::ui::agent_cursor::resume_question_flow(editor, compositor);
                        } else {
                            crate::ui::agent_cursor::show_cursor_request_ui(editor, compositor);
                        }
                    }
                    if editor.agent.open_permission_picker {
                        editor.agent.open_permission_picker = false;
                        crate::ui::agent_permission::show_permission_picker(editor, compositor);
                    }
                })
                .await;
            }
            Ok(())
        });
    }
}

pub fn ensure_session(controller: &AgentController, editor: &mut Editor) {
    if editor.agent.active_session.is_some() {
        return;
    }
    let cwd = editor
        .last_cwd
        .clone()
        .or_else(|| std::env::current_dir().ok());
    controller.send(AgentCommand::StartSession { cwd });
    editor.agent.pending = true;
}

pub fn send_prompt(controller: &AgentController, editor: &mut Editor, text: String) {
    if text.is_empty() {
        return;
    }
    editor.agent.prompt_history.push_front(text.clone());
    editor.agent.history_pos = None;
    editor.agent.input.clear();
    editor.agent.input_cursor = 0;
    editor.agent.pending = true;
    let context = gather_prompt_context(editor);
    controller.send(AgentCommand::SendPrompt { text, context });
}

pub fn gather_prompt_context(editor: &Editor) -> Option<helix_acp::AgentPromptContext> {
    if !editor.agent_settings().include_editor_context {
        return None;
    }

    for (view, _) in editor.tree.views() {
        if editor.tree.is_agent_panel(view.id) {
            continue;
        }
        let doc = editor.documents.get(&view.doc)?;
        let path = doc.path()?.to_path_buf();
        let selection = doc.selection(view.id).primary();
        let text = doc.text();
        let from = selection.from();
        let to = selection.to();
        let selection = if from != to {
            Some(text.slice(from..to).to_string())
        } else {
            None
        };
        return Some(helix_acp::AgentPromptContext {
            file_path: path,
            selection,
        });
    }

    None
}

/// Applies a mock ACP event using the same path as the real runtime listener.
#[cfg(feature = "integration")]
pub fn apply_test_agent_event(editor: &mut Editor, event: AgentEvent) {
    apply_event(editor, &event);
}

fn apply_event(editor: &mut Editor, event: &AgentEvent) {
    let mut needs_redraw = true;
    match event {
        AgentEvent::Initialized {
            agent_name,
            agent_version,
            auth_methods,
            load_session,
            close_session,
        } => {
            let name = agent_name.as_deref().unwrap_or("unknown");
            let version = agent_version.as_deref().unwrap_or("unknown");
            editor.agent.push_debug(format!(
                "ui: initialized agent={name} v={version} auth={auth_methods:?} load_session={load_session} close_session={close_session}"
            ));
            editor.agent.close_session = *close_session;
            editor.agent.status = Some(format!("connected to {name}"));
        }
        AgentEvent::SessionClosed => {
            editor.agent.active_session = None;
            editor.agent.mode = None;
            editor.agent.pending = false;
            editor.agent.cursor_request = None;
            editor.agent.cursor_question_flow = None;
            editor.agent.pending_permission = None;
            editor.agent.status = Some("agent session closed".into());
        }
        AgentEvent::Authenticated { method_id } => {
            editor
                .agent
                .push_debug(format!("ui: authenticated via {method_id}"));
            editor
                .agent
                .status
                .replace(format!("authenticated ({method_id})"));
        }
        AgentEvent::SessionStarted { session_id } => {
            editor.agent.active_session = Some(session_id.0.clone());
            editor.agent.pending = false;
            upsert_session(
                editor,
                AgentSessionMeta {
                    id: session_id.0.clone(),
                    title: None,
                    cwd: editor
                        .last_cwd
                        .clone()
                        .or_else(|| std::env::current_dir().ok())
                        .unwrap_or_else(|| PathBuf::from("/")),
                    updated_at: None,
                },
            );
            editor.agent.status = Some("agent session started".into());
        }
        AgentEvent::SessionLoadStarted { session_id } => {
            editor.agent.active_session = Some(session_id.0.clone());
            editor.agent.pending = true;
            editor.agent.load_replay_count = 0;
            editor.agent.clear_transcript();
            editor.agent.error = None;
            editor.agent.status = Some("loading agent session...".into());
            editor.agent.push_debug(format!("ui: SessionLoadStarted {}", session_id.0));
        }
        AgentEvent::SessionLoaded { session_id } => {
            editor.agent.active_session = Some(session_id.0.clone());
            editor.agent.pending = false;
            editor.agent.status = Some("agent session loaded".into());
            editor.agent.push_debug(format!(
                "ui: SessionLoaded {} ({} replay msgs, {} transcript entries)",
                session_id.0,
                editor.agent.load_replay_count,
                editor.agent.transcript.len()
            ));
            if editor.agent.transcript.is_empty() {
                editor.agent.push_entry(AgentTranscriptEntry::System {
                    text: "No prior messages were replayed for this session.\n\n\
                        Helix received session/load from the agent, but no \
                        user_message_chunk or agent_message_chunk updates arrived.\n\n\
                        Cursor's `agent acp` is known to behave this way: it restores \
                        session state (title, cwd, commands) without streaming prior \
                        transcript text back to the client. This is a Cursor ACP \
                        limitation, not a Helix rendering issue.\n\n\
                        You can continue the loaded session with a new prompt. For full \
                        CLI history, use `agent --resume <session-id>` in a terminal."
                        .into(),
                });
            }
        }
        AgentEvent::SessionList { sessions } => {
            editor.agent.pending = false;
            let mut sessions: Vec<_> = sessions.iter().map(session_meta_from_acp).collect();
            let preferred_cwd = editor
                .last_cwd
                .clone()
                .or_else(|| std::env::current_dir().ok());
            crate::commands::agent::sort_agent_sessions(&mut sessions, preferred_cwd.as_deref());
            editor.agent.set_sessions(sessions);
            if editor.agent.sessions.is_empty() {
                editor.agent.status = Some("no agent sessions found".into());
            } else {
                editor.agent.status = Some(format!(
                    "listed {} agent session(s)",
                    editor.agent.sessions.len()
                ));
            }
        }
        AgentEvent::SessionInfoUpdated { title, updated_at } => {
            apply_active_session_info_update(editor, title.clone(), updated_at.clone());
        }
        AgentEvent::Message(msg) => {
            editor.agent.pending = false;
            editor.agent.load_replay_count = editor.agent.load_replay_count.saturating_add(1);
            match msg {
                AgentMessage::ToolCall {
                    id,
                    title,
                    status,
                    detail,
                } => {
                    editor.agent.upsert_tool_call(
                        id.clone(),
                        title.clone(),
                        status.clone(),
                        detail.clone(),
                    );
                }
                _ => {
                    if let Some(entry) = map_message(msg.clone()) {
                        editor.agent.append_or_push(entry);
                    }
                }
            }
        }
        AgentEvent::ToolCallUpdated {
            id,
            title,
            status,
            detail,
        } => {
            editor
                .agent
                .update_tool_call(id, title.clone(), status.clone(), detail.clone());
        }
        AgentEvent::TurnStarted => {
            editor.agent.pending = true;
        }
        AgentEvent::TurnFinished { stop_reason } => {
            editor.agent.pending = false;
            editor.agent.status = stop_reason.clone();
        }
        AgentEvent::TurnCancelled => {
            editor.agent.pending = false;
            editor.agent.status = Some("turn cancelled".into());
        }
        AgentEvent::ModeUpdated {
            current_mode,
            available_modes,
        } => {
            editor.agent.mode = Some(current_mode.clone());
            editor.agent.available_modes = available_modes
                .iter()
                .map(|mode| AgentModeMeta {
                    id: mode.id.clone(),
                    name: mode.name.clone(),
                    description: mode.description.clone(),
                })
                .collect();
        }
        AgentEvent::Status { text } => {
            editor.agent.status = Some(text.clone());
        }
        AgentEvent::Error { text } => {
            editor.agent.pending = false;
            editor.agent.open_session_picker = false;
            editor.agent.error = Some(text.clone());
            editor
                .agent
                .push_entry(AgentTranscriptEntry::Error { text: text.clone() });
        }
        AgentEvent::PermissionRequested {
            request_id,
            title,
            message,
            options,
        } => {
            editor.agent.pending_permission = Some(AgentPendingPermission {
                request_id: *request_id,
                title: title.clone(),
                message: message.clone(),
                options: options
                    .iter()
                    .map(|option| AgentPermissionOption {
                        id: option.id.clone(),
                        label: option.label.clone(),
                    })
                    .collect(),
            });
            editor.agent.open_permission_picker = true;
        }
        AgentEvent::Debug { text } => {
            editor.agent.push_debug(text);
            needs_redraw = false;
        }
        AgentEvent::CursorRequest {
            request_id,
            method,
            params,
        } => {
            editor.agent.cursor_request = Some(AgentCursorRequest {
                request_id: *request_id,
                method: method.clone(),
                params: params.clone(),
            });
            editor.agent.open_cursor_request = true;
        }
    }
    if needs_redraw {
        helix_event::request_redraw();
    }
}

fn session_meta_from_acp(session: &helix_acp::AgentSessionInfo) -> AgentSessionMeta {
    AgentSessionMeta {
        id: session.id.0.clone(),
        title: session.title.clone(),
        cwd: session.cwd.clone(),
        updated_at: session.updated_at.clone(),
    }
}

fn upsert_session(editor: &mut Editor, session: AgentSessionMeta) {
    if let Some(existing) = editor
        .agent
        .sessions
        .iter_mut()
        .find(|meta| meta.id == session.id)
    {
        if session.title.is_some() {
            existing.title = session.title;
        }
        if !session.cwd.as_os_str().is_empty() {
            existing.cwd = session.cwd;
        }
        existing.updated_at = session.updated_at;
        return;
    }
    editor.agent.sessions.push(session);
}

fn apply_active_session_info_update(
    editor: &mut Editor,
    title: Option<Option<String>>,
    updated_at: Option<Option<String>>,
) {
    let Some(active_session) = editor.agent.active_session.clone() else {
        return;
    };

    if let Some(existing) = editor
        .agent
        .sessions
        .iter_mut()
        .find(|session| session.id == active_session)
    {
        if let Some(title) = title {
            existing.title = title;
        }
        if let Some(updated_at) = updated_at {
            existing.updated_at = updated_at;
        }
        return;
    }

    editor.agent.sessions.push(AgentSessionMeta {
        id: active_session,
        title: title.flatten(),
        cwd: editor
            .last_cwd
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("/")),
        updated_at: updated_at.flatten(),
    });
}

fn map_message(msg: AgentMessage) -> Option<AgentTranscriptEntry> {
    Some(match msg {
        AgentMessage::User { text } if !text.is_empty() => AgentTranscriptEntry::User { text },
        AgentMessage::Assistant { text } if !text.is_empty() => {
            AgentTranscriptEntry::Assistant { text }
        }
        AgentMessage::Thought { text } if !text.is_empty() => {
            AgentTranscriptEntry::Thought { text }
        }
        AgentMessage::ToolCall { .. } => return None,
        AgentMessage::Plan { entries } => AgentTranscriptEntry::Plan { entries },
        AgentMessage::System { text } if !text.is_empty() => AgentTranscriptEntry::System { text },
        AgentMessage::Error { text } if !text.is_empty() => AgentTranscriptEntry::Error { text },
        AgentMessage::User { .. }
        | AgentMessage::Assistant { .. }
        | AgentMessage::Thought { .. }
        | AgentMessage::System { .. }
        | AgentMessage::Error { .. } => return None,
    })
}

pub fn fs_read_impl(editor: &Editor, path: PathBuf) -> FsReadResult {
    let path = helix_stdx::path::canonicalize(path);
    if let Some(doc) = editor.document_by_path(&path) {
        return FsReadResult {
            content: doc.text().to_string(),
        };
    }

    match std::fs::read_to_string(&path) {
        Ok(content) => FsReadResult { content },
        Err(err) => {
            log::warn!("agent fs read failed for {}: {err}", path.display());
            FsReadResult {
                content: String::new(),
            }
        }
    }
}

pub fn fs_write_impl(editor: &mut Editor, request: FsWriteRequest) -> FsWriteResult {
    let path = helix_stdx::path::canonicalize(request.path);

    if let Some(doc_id) = editor.document_id_by_path(&path) {
        let modified = editor
            .documents
            .get(&doc_id)
            .map(|doc| doc.is_modified())
            .unwrap_or(false);
        if modified {
            return FsWriteResult::Conflict {
                message: format!(
                    "{} has unsaved changes; save or discard before agent can write",
                    path.display()
                ),
            };
        }

        let visible_view_id = editor
            .tree
            .views()
            .find_map(|(view, _)| (view.doc == doc_id).then_some(view.id));
        let view_id = visible_view_id
            .or_else(|| {
                editor
                    .documents
                    .get(&doc_id)
                    .and_then(|doc| doc.selections().keys().next().copied())
            })
            .or_else(|| editor.tree.views().next().map(|(view, _)| view.id))
            .unwrap_or_else(ViewId::default);

        let new_rope = Rope::from_str(&request.content);
        let transaction = {
            let doc = editor.documents.get(&doc_id).unwrap();
            diff::compare_ropes(doc.text(), &new_rope)
        };

        if let Err(err) = std::fs::write(&path, &request.content) {
            return FsWriteResult::Error(err.to_string());
        }

        {
            let doc = editor.documents.get_mut(&doc_id).unwrap();
            doc.ensure_view_init(view_id);
            doc.apply(&transaction, view_id);
        }
        if let Some(view_id) = visible_view_id {
            let doc = editor.documents.get_mut(&doc_id).unwrap();
            let view = editor.tree.get_mut(view_id);
            doc.append_changes_to_history(view);
        } else {
            let gutters = editor.config().gutters.clone();
            let doc = editor.documents.get_mut(&doc_id).unwrap();
            let mut view = View::new(doc_id, gutters);
            view.id = view_id;
            doc.append_changes_to_history(&mut view);
        }

        let doc = editor.documents.get_mut(&doc_id).unwrap();
        doc.reset_modified();
        doc.pickup_last_saved_time();
        editor
            .language_servers
            .file_event_handler
            .file_changed(path.clone());
        helix_event::request_redraw();

        return FsWriteResult::Applied;
    }

    match std::fs::write(&path, &request.content) {
        Ok(()) => FsWriteResult::Applied,
        Err(err) => FsWriteResult::Error(err.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use helix_view::agent::AgentTranscriptEntry;

    #[test]
    fn map_message_covers_user_role() {
        let entry = map_message(AgentMessage::User {
            text: "hello".into(),
        })
        .unwrap();
        assert!(matches!(entry, AgentTranscriptEntry::User { .. }));
    }
}
