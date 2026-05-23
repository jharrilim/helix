use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use agent_client_protocol as acp;
use agent_client_protocol::schema::{
    AgentNotification, AuthMethod, CancelNotification, ClientCapabilities, ContentBlock,
    FileSystemCapabilities, Implementation, InitializeRequest, InitializeResponse,
    ListSessionsRequest, LoadSessionRequest, NewSessionRequest, PromptRequest, ProtocolVersion,
    ReadTextFileRequest, ReadTextFileResponse, RequestPermissionOutcome, RequestPermissionRequest,
    RequestPermissionResponse, SelectedPermissionOutcome, SessionInfo, SessionModeState,
    SessionNotification, SessionUpdate, SetSessionModeRequest, TextContent, WriteTextFileRequest,
    WriteTextFileResponse,
};
use anyhow::{anyhow, Context as _};
use parking_lot::Mutex;
use serde_json::{json, Value};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;

use crate::cursor::{
    CursorExtensionNotification, CursorExtensionRequest, METHOD_CREATE_PLAN, METHOD_ASK_QUESTION,
    METHOD_GENERATE_IMAGE, METHOD_TASK, METHOD_UPDATE_TODOS,
};
use crate::events::{AgentCommand, AgentEvent, AgentMessage, AgentModeInfo};
use crate::fs::{FsReadResult, FsWriteRequest, FsWriteResult};
use crate::mcp_config::resolve_mcp_servers;
use crate::session::{AgentSessionId, AgentSessionInfo};

pub type FsReadFn = Arc<dyn Fn(PathBuf) -> FsReadResult + Send + Sync>;
pub type FsWriteFn = Arc<dyn Fn(FsWriteRequest) -> FsWriteResult + Send + Sync>;

/// Configuration for spawning a local ACP agent process.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Command string understood by `AcpAgent::from_str`.
    pub command: String,
    pub client_name: String,
    pub client_version: String,
    /// Preferred auth method id (e.g. `cursor_login`).
    pub auth_method: Option<String>,
    /// Skip the authenticate RPC even when the agent advertises auth methods.
    pub skip_authenticate: bool,
    /// Default session mode to apply after session/new or session/load.
    pub default_mode: Option<String>,
    /// Optional override path to an MCP config file.
    pub mcp_config_path: Option<PathBuf>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            command: String::new(),
            client_name: "helix".to_string(),
            client_version: env!("CARGO_PKG_VERSION").to_string(),
            auth_method: None,
            skip_authenticate: false,
            default_mode: None,
            mcp_config_path: None,
        }
    }
}

/// Handle for sending commands to a running agent runtime.
#[derive(Clone)]
pub struct AgentRuntimeHandle {
    cmd_tx: UnboundedSender<AgentCommand>,
}

impl AgentRuntimeHandle {
    pub fn send(&self, cmd: AgentCommand) {
        let _ = self.cmd_tx.send(cmd);
    }
}

/// Background ACP client runtime.
pub struct AgentRuntime;

impl AgentRuntime {
    pub fn spawn(
        config: AgentConfig,
        fs_read: FsReadFn,
        fs_write: FsWriteFn,
    ) -> (AgentRuntimeHandle, UnboundedReceiver<AgentEvent>) {
        let (events_tx, events_rx) = unbounded_channel();
        let (cmd_tx, cmd_rx) = unbounded_channel();

        tokio::task::spawn(async move {
            if let Err(err) =
                run_runtime(config, fs_read, fs_write, events_tx.clone(), cmd_rx).await
            {
                let _ = events_tx.send(AgentEvent::Error {
                    text: err.to_string(),
                });
            }
        });

        (AgentRuntimeHandle { cmd_tx }, events_rx)
    }
}

type CursorResponder = oneshot::Sender<Value>;

async fn run_runtime(
    config: AgentConfig,
    fs_read: FsReadFn,
    fs_write: FsWriteFn,
    events_tx: UnboundedSender<AgentEvent>,
    mut cmd_rx: UnboundedReceiver<AgentCommand>,
) -> anyhow::Result<()> {
    if config.command.is_empty() {
        return Err(anyhow!("agent command is not configured"));
    }

    let agent =
        acp::AcpAgent::from_str(&config.command).context("failed to parse agent command")?;

    let events = events_tx.clone();
    let fs_read_cb = fs_read.clone();
    let fs_write_cb = fs_write.clone();
    let pending_cursor: Arc<Mutex<HashMap<u64, CursorResponder>>> = Arc::new(Mutex::new(HashMap::new()));
    let next_cursor_id: Arc<Mutex<u64>> = Arc::new(Mutex::new(0));

    acp::Client
        .builder()
        .on_receive_notification(
            move |notification: SessionNotification, _cx| {
                let events = events.clone();
                async move {
                    send_session_update(&events, "session/update", notification.update);
                    Ok(())
                }
            },
            acp::on_receive_notification!(),
        )
        .on_receive_notification(
            {
                let events = events_tx.clone();
                move |notification: AgentNotification, _cx| {
                    let events = events.clone();
                    async move {
                        match notification {
                            AgentNotification::SessionNotification(notification) => {
                                send_session_update(&events, "sessionUpdate", notification.update);
                            }
                            other => {
                                let _ = events.send(AgentEvent::Debug {
                                    text: format!("acp[agentNotification]: ignored {other:?}"),
                                });
                                log::debug!("ignoring ACP agent notification: {other:?}");
                            }
                        }
                        Ok(())
                    }
                }
            },
            acp::on_receive_notification!(),
        )
        .on_receive_notification(
            {
                let events = events_tx.clone();
                move |notification: CursorExtensionNotification, _cx| {
                    let events = events.clone();
                    async move {
                        handle_cursor_notification(&events, &notification.method, notification.params);
                        Ok(())
                    }
                }
            },
            acp::on_receive_notification!(),
        )
        .on_receive_request(
            {
                let fs_read = fs_read_cb.clone();
                move |request: ReadTextFileRequest,
                      responder: acp::Responder<ReadTextFileResponse>,
                      _connection| {
                    let fs_read = fs_read.clone();
                    async move {
                        let result = fs_read(request.path);
                        responder.respond(ReadTextFileResponse::new(result.content))
                    }
                }
            },
            acp::on_receive_request!(),
        )
        .on_receive_request(
            {
                let fs_write = fs_write_cb.clone();
                move |request: WriteTextFileRequest,
                      responder: acp::Responder<WriteTextFileResponse>,
                      _connection| {
                    let fs_write = fs_write.clone();
                    async move {
                        let write_result = fs_write(FsWriteRequest {
                            path: request.path,
                            content: request.content,
                        });
                        match write_result {
                            FsWriteResult::Applied => {
                                responder.respond(WriteTextFileResponse::new())
                            }
                            FsWriteResult::Conflict { message } => {
                                responder.respond_with_internal_error(message)
                            }
                            FsWriteResult::Rejected => {
                                responder.respond_with_internal_error("write rejected by user")
                            }
                            FsWriteResult::Error(err) => responder.respond_with_internal_error(err),
                        }
                    }
                }
            },
            acp::on_receive_request!(),
        )
        .on_receive_request(
            move |request: RequestPermissionRequest,
                  responder: acp::Responder<RequestPermissionResponse>,
                  _connection| async move {
                let option_id = pick_permission_option(&request);
                if let Some(id) = option_id {
                    responder.respond(RequestPermissionResponse::new(
                        RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(id)),
                    ))
                } else {
                    responder.respond(RequestPermissionResponse::new(
                        RequestPermissionOutcome::Cancelled,
                    ))
                }
            },
            acp::on_receive_request!(),
        )
        .on_receive_request(
            {
                let events = events_tx.clone();
                let pending_cursor = pending_cursor.clone();
                let next_cursor_id = next_cursor_id.clone();
                move |request: CursorExtensionRequest,
                      responder: acp::Responder<Value>,
                      _connection| {
                    let events = events.clone();
                    let pending_cursor = pending_cursor.clone();
                    let next_cursor_id = next_cursor_id.clone();
                    async move {
                        if is_cursor_notification_method(&request.method) {
                            handle_cursor_notification(&events, &request.method, request.params.clone());
                            responder.respond(json!({}))
                        } else if request.method == METHOD_ASK_QUESTION
                            || request.method == METHOD_CREATE_PLAN
                        {
                            let request_id = {
                                let mut id = next_cursor_id.lock();
                                *id += 1;
                                *id
                            };
                            let (tx, rx) = oneshot::channel();
                            pending_cursor.lock().insert(request_id, tx);
                            let _ = events.send(AgentEvent::CursorRequest {
                                request_id,
                                method: request.method.clone(),
                                params: request.params.clone(),
                            });
                            match rx.await {
                                Ok(result) => responder.respond(result),
                                Err(_) => responder.respond(json!({
                                    "outcome": { "outcome": "cancelled" }
                                })),
                            }
                        } else {
                            handle_cursor_notification(&events, &request.method, request.params.clone());
                            responder.respond(json!({}))
                        }
                    }
                }
            },
            acp::on_receive_request!(),
        )
        .connect_with(agent, move |connection: acp::ConnectionTo<acp::Agent>| {
            let events_tx = events_tx.clone();
            let config = config.clone();
            let pending_cursor = pending_cursor.clone();
            async move {
                let capabilities = ClientCapabilities::new().fs(FileSystemCapabilities::new()
                    .read_text_file(true)
                    .write_text_file(true));

                let init_response = connection
                    .send_request(
                        InitializeRequest::new(ProtocolVersion::V1)
                            .client_capabilities(capabilities)
                            .client_info(Implementation::new(
                                config.client_name.clone(),
                                config.client_version.clone(),
                            )),
                    )
                    .block_task()
                    .await?;

                emit_initialized(&events_tx, &init_response);
                authenticate(&connection, &events_tx, &config, &init_response).await?;

                let active_session: Arc<Mutex<Option<acp::schema::SessionId>>> =
                    Arc::new(Mutex::new(None));

                while let Some(cmd) = cmd_rx.recv().await {
                    match cmd {
                        AgentCommand::StartSession { cwd } => {
                            let cwd = cwd.unwrap_or_else(|| {
                                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))
                            });
                            let mcp_servers = resolve_mcp_servers(
                                &cwd,
                                config.mcp_config_path.as_deref(),
                            );
                            if !mcp_servers.is_empty() {
                                let names: Vec<_> = mcp_servers
                                    .iter()
                                    .filter_map(|server| match server {
                                        acp::schema::McpServer::Stdio(s) => Some(s.name.clone()),
                                        _ => None,
                                    })
                                    .collect();
                                let _ = events_tx.send(AgentEvent::Debug {
                                    text: format!("runtime: mcp servers: {}", names.join(", ")),
                                });
                            }
                            let response = connection
                                .send_request(
                                    NewSessionRequest::new(cwd.clone()).mcp_servers(mcp_servers),
                                )
                                .block_task()
                                .await?;
                            let id = AgentSessionId(response.session_id.to_string());
                            *active_session.lock() = Some(response.session_id.clone());
                            apply_mode_from_response(
                                &connection,
                                &events_tx,
                                &config,
                                response.session_id.clone(),
                                response.modes,
                            )
                            .await?;
                            let _ = events_tx.send(AgentEvent::SessionStarted { session_id: id });
                        }
                        AgentCommand::LoadSession { session_id, cwd } => {
                            let cwd = cwd.unwrap_or_else(|| {
                                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))
                            });
                            let sid: acp::schema::SessionId = session_id.0.clone().into();
                            *active_session.lock() = Some(sid.clone());
                            let _ = events_tx.send(AgentEvent::Debug {
                                text: format!(
                                    "runtime: LoadSession {} cwd={}",
                                    session_id.0,
                                    cwd.display()
                                ),
                            });
                            let _ = events_tx.send(AgentEvent::SessionLoadStarted {
                                session_id: session_id.clone(),
                            });
                            let mcp_servers = resolve_mcp_servers(
                                &cwd,
                                config.mcp_config_path.as_deref(),
                            );
                            let response = connection
                                .send_request(
                                    LoadSessionRequest::new(sid.clone(), cwd)
                                        .mcp_servers(mcp_servers),
                                )
                                .block_task()
                                .await?;
                            apply_mode_from_response(
                                &connection,
                                &events_tx,
                                &config,
                                sid.clone(),
                                response.modes,
                            )
                            .await?;
                            let _ = events_tx.send(AgentEvent::Debug {
                                text: format!(
                                    "runtime: LoadSession RPC complete for {}",
                                    session_id.0
                                ),
                            });
                            let _ = events_tx.send(AgentEvent::SessionLoaded { session_id });
                        }
                        AgentCommand::ListSessions { cwd } => {
                            match list_sessions(&connection, cwd).await {
                                Ok(sessions) => {
                                    let _ = events_tx.send(AgentEvent::SessionList { sessions });
                                }
                                Err(err) => {
                                    let _ = events_tx.send(AgentEvent::Error {
                                        text: err.to_string(),
                                    });
                                }
                            }
                        }
                        AgentCommand::SendPrompt { text } => {
                            let sid = active_session
                                .lock()
                                .clone()
                                .ok_or_else(|| anyhow!("no active agent session"))?;
                            let _ = events_tx.send(AgentEvent::Message(AgentMessage::User {
                                text: text.clone(),
                            }));
                            let _ = events_tx.send(AgentEvent::TurnStarted);
                            let response = connection
                                .send_request(PromptRequest::new(
                                    sid,
                                    vec![ContentBlock::Text(TextContent::new(text))],
                                ))
                                .block_task()
                                .await?;
                            let _ = events_tx.send(AgentEvent::TurnFinished {
                                stop_reason: Some(format!("{:?}", response.stop_reason)),
                            });
                        }
                        AgentCommand::Cancel => {
                            if let Some(sid) = active_session.lock().clone() {
                                connection.send_notification(CancelNotification::new(sid))?;
                                let _ = events_tx.send(AgentEvent::TurnCancelled);
                                let _ = events_tx.send(AgentEvent::Status {
                                    text: "agent turn cancelled".into(),
                                });
                            }
                        }
                        AgentCommand::Stop => {
                            active_session.lock().take();
                            let _ = events_tx.send(AgentEvent::Status {
                                text: "agent session stopped".into(),
                            });
                        }
                        AgentCommand::SetMode { mode_id } => {
                            if let Some(sid) = active_session.lock().clone() {
                                connection
                                    .send_request(SetSessionModeRequest::new(
                                        sid,
                                        mode_id.clone(),
                                    ))
                                    .block_task()
                                    .await?;
                                let _ = events_tx.send(AgentEvent::Status {
                                    text: format!("agent mode set to {mode_id}"),
                                });
                            }
                        }
                        AgentCommand::RespondPermission { .. } => {}
                        AgentCommand::RespondCursor { request_id, result } => {
                            if let Some(tx) = pending_cursor.lock().remove(&request_id) {
                                let _ = tx.send(result);
                            }
                        }
                    }
                }

                Ok(())
            }
        })
        .await?;

    Ok(())
}

fn emit_initialized(events: &UnboundedSender<AgentEvent>, response: &InitializeResponse) {
    let agent_name = response.agent_info.as_ref().map(|info| info.name.clone());
    let agent_version = response
        .agent_info
        .as_ref()
        .map(|info| info.version.clone());
    let auth_methods = response
        .auth_methods
        .iter()
        .map(auth_method_id)
        .collect();
    let load_session = response.agent_capabilities.load_session;
    let _ = events.send(AgentEvent::Initialized {
        agent_name,
        agent_version,
        auth_methods,
        load_session,
    });
}

async fn authenticate(
    connection: &acp::ConnectionTo<acp::Agent>,
    events: &UnboundedSender<AgentEvent>,
    config: &AgentConfig,
    init_response: &InitializeResponse,
) -> anyhow::Result<()> {
    if config.skip_authenticate || init_response.auth_methods.is_empty() {
        let _ = events.send(AgentEvent::Debug {
            text: "runtime: skipping authenticate".into(),
        });
        return Ok(());
    }

    let method_id = pick_auth_method(&init_response.auth_methods, config.auth_method.as_deref())
        .ok_or_else(|| anyhow!("agent requires authentication but no auth method is available"))?;

    let _ = events.send(AgentEvent::Debug {
        text: format!("runtime: authenticate method={method_id}"),
    });

    match connection
        .send_request(acp::schema::AuthenticateRequest::new(method_id.clone()))
        .block_task()
        .await
    {
        Ok(_) => {
            let _ = events.send(AgentEvent::Authenticated { method_id });
            Ok(())
        }
        Err(err) => {
            let _ = events.send(AgentEvent::Error {
                text: format!("agent authentication failed: {err}"),
            });
            Ok(())
        }
    }
}

fn pick_auth_method(methods: &[AuthMethod], preferred: Option<&str>) -> Option<String> {
    if let Some(preferred) = preferred {
        if methods.iter().any(|method| auth_method_id(method) == preferred) {
            return Some(preferred.to_string());
        }
    }
    if methods
        .iter()
        .any(|method| auth_method_id(method) == "cursor_login")
    {
        return Some("cursor_login".to_string());
    }
    methods.first().map(auth_method_id)
}

fn auth_method_id(method: &AuthMethod) -> String {
    method.id().to_string()
}

async fn apply_mode_from_response(
    connection: &acp::ConnectionTo<acp::Agent>,
    events: &UnboundedSender<AgentEvent>,
    config: &AgentConfig,
    session_id: acp::schema::SessionId,
    modes: Option<SessionModeState>,
) -> anyhow::Result<()> {
    let Some(modes) = modes else {
        return Ok(());
    };

    emit_mode_state(events, &modes);

    if let Some(default_mode) = config.default_mode.as_deref() {
        let available = modes
            .available_modes
            .iter()
            .any(|mode| mode.id.to_string() == default_mode);
        if available && modes.current_mode_id.to_string() != default_mode {
            connection
                .send_request(SetSessionModeRequest::new(
                    session_id,
                    default_mode.to_string(),
                ))
                .block_task()
                .await?;
            let _ = events.send(AgentEvent::Status {
                text: format!("agent mode set to {default_mode}"),
            });
        }
    }

    Ok(())
}

fn emit_mode_state(events: &UnboundedSender<AgentEvent>, modes: &SessionModeState) {
    let available_modes = modes
        .available_modes
        .iter()
        .map(|mode| AgentModeInfo {
            id: mode.id.to_string(),
            name: mode.name.clone(),
            description: mode.description.clone(),
        })
        .collect();
    let _ = events.send(AgentEvent::ModeUpdated {
        current_mode: modes.current_mode_id.to_string(),
        available_modes,
    });
}

fn pick_permission_option(request: &RequestPermissionRequest) -> Option<acp::schema::PermissionOptionId> {
    for preferred in ["allow-once", "allow-always", "reject-once"] {
        if let Some(option) = request
            .options
            .iter()
            .find(|opt| opt.option_id.to_string() == preferred)
        {
            return Some(option.option_id.clone());
        }
    }
    request.options.first().map(|opt| opt.option_id.clone())
}

fn is_cursor_notification_method(method: &str) -> bool {
    matches!(
        method,
        METHOD_UPDATE_TODOS | METHOD_TASK | METHOD_GENERATE_IMAGE
    )
}

fn handle_cursor_notification(events: &UnboundedSender<AgentEvent>, method: &str, params: Value) {
    match method {
        METHOD_UPDATE_TODOS => {
            if let Ok(request) =
                serde_json::from_value::<crate::cursor::CursorUpdateTodosRequest>(params)
            {
                let entries = request
                    .todos
                    .iter()
                    .map(|todo| format!("[{}] {}", todo.status_label(), todo.content))
                    .collect();
                let _ = events.send(AgentEvent::Message(AgentMessage::Plan { entries }));
            }
        }
        METHOD_TASK => {
            if let Ok(request) =
                serde_json::from_value::<crate::cursor::CursorTaskRequest>(params)
            {
                let detail = request
                    .agent_id
                    .map(|id| format!("subagent={id}"))
                    .or_else(|| request.duration_ms.map(|ms| format!("duration={ms}ms")));
                let _ = events.send(AgentEvent::Message(AgentMessage::ToolCall {
                    name: request.description,
                    status: format!("{:?}", request.subagent_type),
                    detail,
                }));
            }
        }
        METHOD_GENERATE_IMAGE => {
            if let Ok(request) =
                serde_json::from_value::<crate::cursor::CursorGenerateImageRequest>(params)
            {
                let mut text = request.description;
                if let Some(path) = request.file_path {
                    text.push_str(&format!("\nfile: {path}"));
                }
                let _ = events.send(AgentEvent::Message(AgentMessage::System { text }));
            }
        }
        other => {
            let _ = events.send(AgentEvent::Debug {
                text: format!("cursor notification ignored: {other}"),
            });
        }
    }
}

trait CursorTodoStatusLabel {
    fn status_label(&self) -> &'static str;
}

impl CursorTodoStatusLabel for crate::cursor::CursorTodo {
    fn status_label(&self) -> &'static str {
        match self.status {
            crate::cursor::CursorTodoStatus::Pending => "pending",
            crate::cursor::CursorTodoStatus::InProgress => "in_progress",
            crate::cursor::CursorTodoStatus::Completed => "completed",
            crate::cursor::CursorTodoStatus::Cancelled => "cancelled",
        }
    }
}

async fn list_sessions(
    connection: &acp::ConnectionTo<acp::Agent>,
    cwd: Option<PathBuf>,
) -> anyhow::Result<Vec<AgentSessionInfo>> {
    let mut sessions = Vec::new();
    let mut cursor = None;

    loop {
        let mut request = ListSessionsRequest::new();
        if let Some(cwd) = cwd.as_ref() {
            request = request.cwd(cwd.clone());
        }
        if let Some(cursor) = cursor.take() {
            request = request.cursor(cursor);
        }

        let response = connection.send_request(request).block_task().await?;

        sessions.extend(response.sessions.into_iter().map(map_session_info));
        cursor = response.next_cursor;
        if cursor.is_none() {
            break;
        }
    }

    Ok(sessions)
}

fn map_session_info(info: SessionInfo) -> AgentSessionInfo {
    AgentSessionInfo {
        id: AgentSessionId(info.session_id.to_string()),
        title: info.title,
        cwd: info.cwd,
        updated_at: info.updated_at,
    }
}

fn send_session_update(events: &UnboundedSender<AgentEvent>, source: &str, update: SessionUpdate) {
    let summary = session_update_summary(&update);
    let _ = events.send(AgentEvent::Debug {
        text: format!("acp[{source}]: {summary}"),
    });
    if let Some(event) = map_session_update(update) {
        let _ = events.send(event);
    }
}

fn session_update_summary(update: &SessionUpdate) -> String {
    match update {
        SessionUpdate::UserMessageChunk(chunk) => format!(
            "UserMessageChunk text_len={}",
            content_block_text(&chunk.content).len()
        ),
        SessionUpdate::AgentMessageChunk(chunk) => format!(
            "AgentMessageChunk text_len={}",
            content_block_text(&chunk.content).len()
        ),
        SessionUpdate::AgentThoughtChunk(chunk) => format!(
            "AgentThoughtChunk text_len={}",
            content_block_text(&chunk.content).len()
        ),
        SessionUpdate::ToolCall(call) => format!("ToolCall title={}", call.title),
        SessionUpdate::Plan(plan) => format!("Plan entries={}", plan.entries.len()),
        SessionUpdate::SessionInfoUpdate(info) => format!(
            "SessionInfoUpdate title={:?}",
            info.title.as_opt_ref()
        ),
        other => format!("unmapped {other:?}"),
    }
}

fn map_session_update(update: SessionUpdate) -> Option<AgentEvent> {
    Some(match update {
        SessionUpdate::UserMessageChunk(chunk) => AgentEvent::Message(AgentMessage::User {
            text: content_block_text(&chunk.content),
        }),
        SessionUpdate::AgentMessageChunk(chunk) => AgentEvent::Message(AgentMessage::Assistant {
            text: content_block_text(&chunk.content),
        }),
        SessionUpdate::AgentThoughtChunk(chunk) => AgentEvent::Message(AgentMessage::Thought {
            text: content_block_text(&chunk.content),
        }),
        SessionUpdate::ToolCall(call) => AgentEvent::Message(AgentMessage::ToolCall {
            name: call.title,
            status: format!("{:?}", call.status),
            detail: if call.content.is_empty() {
                None
            } else {
                Some(format!("{:?}", call.content))
            },
        }),
        SessionUpdate::Plan(plan) => AgentEvent::Message(AgentMessage::Plan {
            entries: plan.entries.into_iter().map(|e| e.content).collect(),
        }),
        SessionUpdate::SessionInfoUpdate(info) => AgentEvent::SessionInfoUpdated {
            title: info.title.as_opt_ref().map(|title| title.cloned()),
            updated_at: info
                .updated_at
                .as_opt_ref()
                .map(|updated_at| updated_at.cloned()),
        },
        other => {
            log::debug!("ignoring ACP session update: {other:?}");
            return None;
        }
    })
}

fn content_block_text(block: &ContentBlock) -> String {
    match block {
        ContentBlock::Text(text) => text.text.clone(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::{SessionId, SessionInfo};

    #[test]
    fn content_block_text_extracts_plain_text() {
        let block = ContentBlock::Text(TextContent::new("hello"));
        assert_eq!(content_block_text(&block), "hello");
    }

    #[test]
    fn map_session_info_maps_acp_fields() {
        let info = SessionInfo::new(SessionId::new("sess-1"), "/tmp/project")
            .title("My session")
            .updated_at("2026-01-01T00:00:00Z");
        let mapped = map_session_info(info);
        assert_eq!(mapped.id.0, "sess-1");
        assert_eq!(mapped.title.as_deref(), Some("My session"));
        assert_eq!(mapped.cwd, PathBuf::from("/tmp/project"));
        assert_eq!(mapped.updated_at.as_deref(), Some("2026-01-01T00:00:00Z"));
    }

    #[test]
    fn pick_permission_prefers_allow_once() {
        use agent_client_protocol::schema::{
            PermissionOption, PermissionOptionId, PermissionOptionKind, RequestPermissionRequest,
            SessionId, ToolCallId, ToolCallUpdate, ToolCallUpdateFields,
        };

        let request = RequestPermissionRequest::new(
            SessionId::new("s1"),
            ToolCallUpdate::new(ToolCallId::new("call-1"), ToolCallUpdateFields::new()),
            vec![
                PermissionOption::new(
                    PermissionOptionId::new("reject-once"),
                    "Reject",
                    PermissionOptionKind::RejectOnce,
                ),
                PermissionOption::new(
                    PermissionOptionId::new("allow-once"),
                    "Allow once",
                    PermissionOptionKind::AllowOnce,
                ),
            ],
        );
        assert_eq!(
            pick_permission_option(&request).unwrap().to_string(),
            "allow-once"
        );
    }
}
