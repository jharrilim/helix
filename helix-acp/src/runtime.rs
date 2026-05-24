use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use agent_client_protocol as acp;
use agent_client_protocol::schema::{
    AgentNotification, AuthMethod, CancelNotification, ClientCapabilities, CloseSessionRequest,
    ContentBlock, FileSystemCapabilities, Implementation, InitializeRequest, InitializeResponse,
    ListSessionsRequest, LoadSessionRequest, NewSessionRequest, PromptRequest, ProtocolVersion,
    ReadTextFileRequest, ReadTextFileResponse, RequestPermissionOutcome, RequestPermissionRequest,
    RequestPermissionResponse, ResourceLink, SelectedPermissionOutcome, SessionInfo, SessionModeState,
    SessionNotification, SessionUpdate, SetSessionModeRequest, TextContent, ToolCall,
    ToolCallContent, ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields, WriteTextFileRequest,
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
use crate::events::{AgentCommand, AgentEvent, AgentMessage, AgentModeInfo, AgentPermissionOption, AgentPromptContext};
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
    /// Emit verbose debug events to the UI layer.
    pub debug_logging: bool,
    /// Auto-approve permission requests without UI interaction.
    pub auto_approve_permissions: bool,
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
            debug_logging: false,
            auto_approve_permissions: false,
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
type PermissionResponder = oneshot::Sender<Option<String>>;

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

    let debug_logging = config.debug_logging;
    let auto_approve_permissions = config.auto_approve_permissions;

    let events = events_tx.clone();
    let fs_read_cb = fs_read.clone();
    let fs_write_cb = fs_write.clone();
    let pending_cursor: Arc<Mutex<HashMap<u64, CursorResponder>>> = Arc::new(Mutex::new(HashMap::new()));
    let next_cursor_id: Arc<Mutex<u64>> = Arc::new(Mutex::new(0));
    let pending_permission: Arc<Mutex<HashMap<u64, PermissionResponder>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let next_permission_id: Arc<Mutex<u64>> = Arc::new(Mutex::new(0));

    acp::Client
        .builder()
        .on_receive_notification(
            move |notification: SessionNotification, _cx| {
                let events = events.clone();
                async move {
                    send_session_update(&events, "session/update", notification.update, debug_logging);
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
                                send_session_update(
                                    &events,
                                    "sessionUpdate",
                                    notification.update,
                                    debug_logging,
                                );
                            }
                            other => {
                                emit_debug(
                                    &events,
                                    debug_logging,
                                    format!("acp[agentNotification]: ignored {other:?}"),
                                );
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
                        handle_cursor_notification(&events, debug_logging, &notification.method, notification.params);
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
            {
                let events = events_tx.clone();
                let pending_permission = pending_permission.clone();
                let next_permission_id = next_permission_id.clone();
                move |request: RequestPermissionRequest,
                      responder: acp::Responder<RequestPermissionResponse>,
                      _connection| {
                    let events = events.clone();
                    let pending_permission = pending_permission.clone();
                    let next_permission_id = next_permission_id.clone();
                    async move {
                        if auto_approve_permissions {
                            respond_permission_auto(&request, responder);
                            return Ok(());
                        }

                        let request_id = {
                            let mut id = next_permission_id.lock();
                            *id += 1;
                            *id
                        };
                        let (tx, rx) = oneshot::channel();
                        pending_permission.lock().insert(request_id, tx);
                        let title = permission_title(&request);
                        let message = permission_message(&request.tool_call);
                        let options = request
                            .options
                            .iter()
                            .map(|option| AgentPermissionOption {
                                id: option.option_id.to_string(),
                                label: option.name.clone(),
                            })
                            .collect();
                        let _ = events.send(AgentEvent::PermissionRequested {
                            request_id,
                            title,
                            message,
                            options,
                        });
                        match rx.await {
                            Ok(Some(option_id)) => {
                                responder.respond(RequestPermissionResponse::new(
                                    RequestPermissionOutcome::Selected(
                                        SelectedPermissionOutcome::new(option_id),
                                    ),
                                ))?;
                            }
                            _ => {
                                responder.respond(RequestPermissionResponse::new(
                                    RequestPermissionOutcome::Cancelled,
                                ))?;
                            }
                        }
                        Ok(())
                    }
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
                            handle_cursor_notification(
                                &events,
                                debug_logging,
                                &request.method,
                                request.params.clone(),
                            );
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
                            handle_cursor_notification(
                                &events,
                                debug_logging,
                                &request.method,
                                request.params.clone(),
                            );
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
            let pending_permission = pending_permission.clone();
            let debug_logging = debug_logging;
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
                let close_session = init_response
                    .agent_capabilities
                    .session_capabilities
                    .close
                    .is_some();
                authenticate(&connection, &events_tx, &config, &init_response, debug_logging).await?;

                let active_session: Arc<Mutex<Option<acp::schema::SessionId>>> =
                    Arc::new(Mutex::new(None));
                let close_session_cap = Arc::new(close_session);

                while let Some(cmd) = cmd_rx.recv().await {
                    match cmd {
                        AgentCommand::StartSession { cwd } => {
                            start_new_session(
                                &connection,
                                &events_tx,
                                &config,
                                &active_session,
                                cwd,
                                debug_logging,
                            )
                            .await?;
                        }
                        AgentCommand::LoadSession { session_id, cwd } => {
                            let cwd = cwd.unwrap_or_else(|| {
                                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))
                            });
                            let sid: acp::schema::SessionId = session_id.0.clone().into();
                            *active_session.lock() = Some(sid.clone());
                            emit_debug(
                                &events_tx,
                                debug_logging,
                                format!(
                                    "runtime: LoadSession {} cwd={}",
                                    session_id.0,
                                    cwd.display()
                                ),
                            );
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
                            emit_debug(
                                &events_tx,
                                debug_logging,
                                format!(
                                    "runtime: LoadSession RPC complete for {}",
                                    session_id.0
                                ),
                            );
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
                        AgentCommand::SendPrompt { text, context } => {
                            let sid = active_session
                                .lock()
                                .clone()
                                .ok_or_else(|| anyhow!("no active agent session"))?;
                            let _ = events_tx.send(AgentEvent::Message(AgentMessage::User {
                                text: text.clone(),
                            }));
                            let _ = events_tx.send(AgentEvent::TurnStarted);
                            let blocks = build_prompt_blocks(text, context);
                            let response = connection
                                .send_request(PromptRequest::new(sid, blocks))
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
                        AgentCommand::CloseSession => {
                            close_active_session(
                                &connection,
                                &events_tx,
                                &active_session,
                                &close_session_cap,
                            )
                            .await?;
                        }
                        AgentCommand::NewSession { cwd } => {
                            close_active_session(
                                &connection,
                                &events_tx,
                                &active_session,
                                &close_session_cap,
                            )
                            .await?;
                            start_new_session(
                                &connection,
                                &events_tx,
                                &config,
                                &active_session,
                                cwd,
                                debug_logging,
                            )
                            .await?;
                        }
                        AgentCommand::Stop => {
                            active_session.lock().take();
                            let _ = events_tx.send(AgentEvent::Status {
                                text: "agent session stopped".into(),
                            });
                        }
                        AgentCommand::SetMode { mode_id } => {
                            let sid = active_session.lock().clone();
                            if let Some(sid) = sid {
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
                        AgentCommand::RespondPermission {
                            request_id,
                            option_id,
                        } => {
                            if let Some(tx) = pending_permission.lock().remove(&request_id) {
                                let _ = tx.send(option_id);
                            }
                        }
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
    let close_session = response
        .agent_capabilities
        .session_capabilities
        .close
        .is_some();
    let _ = events.send(AgentEvent::Initialized {
        agent_name,
        agent_version,
        auth_methods,
        load_session,
        close_session,
    });
}

async fn close_active_session(
    connection: &acp::ConnectionTo<acp::Agent>,
    events: &UnboundedSender<AgentEvent>,
    active_session: &Arc<Mutex<Option<acp::schema::SessionId>>>,
    close_session_cap: &Arc<bool>,
) -> anyhow::Result<()> {
    let Some(sid) = active_session.lock().take() else {
        return Ok(());
    };
    if **close_session_cap {
        connection
            .send_request(CloseSessionRequest::new(sid))
            .block_task()
            .await?;
    }
    let _ = events.send(AgentEvent::SessionClosed);
    Ok(())
}

async fn start_new_session(
    connection: &acp::ConnectionTo<acp::Agent>,
    events: &UnboundedSender<AgentEvent>,
    config: &AgentConfig,
    active_session: &Arc<Mutex<Option<acp::schema::SessionId>>>,
    cwd: Option<PathBuf>,
    debug_logging: bool,
) -> anyhow::Result<()> {
    let cwd = cwd.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));
    let mcp_servers = resolve_mcp_servers(&cwd, config.mcp_config_path.as_deref());
    if !mcp_servers.is_empty() {
        let names: Vec<_> = mcp_servers
            .iter()
            .filter_map(|server| match server {
                acp::schema::McpServer::Stdio(s) => Some(s.name.clone()),
                _ => None,
            })
            .collect();
        emit_debug(
            events,
            debug_logging,
            format!("runtime: mcp servers: {}", names.join(", ")),
        );
    }
    let response = connection
        .send_request(NewSessionRequest::new(cwd.clone()).mcp_servers(mcp_servers))
        .block_task()
        .await?;
    let id = AgentSessionId(response.session_id.to_string());
    *active_session.lock() = Some(response.session_id.clone());
    apply_mode_from_response(
        connection,
        events,
        config,
        response.session_id.clone(),
        response.modes,
    )
    .await?;
    let _ = events.send(AgentEvent::SessionStarted { session_id: id });
    Ok(())
}

async fn authenticate(
    connection: &acp::ConnectionTo<acp::Agent>,
    events: &UnboundedSender<AgentEvent>,
    config: &AgentConfig,
    init_response: &InitializeResponse,
    debug_logging: bool,
) -> anyhow::Result<()> {
    if config.skip_authenticate || init_response.auth_methods.is_empty() {
        emit_debug(events, debug_logging, "runtime: skipping authenticate");
        return Ok(());
    }

    let method_id = pick_auth_method(&init_response.auth_methods, config.auth_method.as_deref())
        .ok_or_else(|| anyhow!("agent requires authentication but no auth method is available"))?;

    emit_debug(
        events,
        debug_logging,
        format!("runtime: authenticate method={method_id}"),
    );

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

fn respond_permission_auto(
    request: &RequestPermissionRequest,
    responder: acp::Responder<RequestPermissionResponse>,
) {
    if let Some(option_id) = pick_permission_option(request) {
        let _ = responder.respond(RequestPermissionResponse::new(
            RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(option_id)),
        ));
    } else {
        let _ = responder.respond(RequestPermissionResponse::new(
            RequestPermissionOutcome::Cancelled,
        ));
    }
}

fn permission_title(request: &RequestPermissionRequest) -> String {
    request
        .tool_call
        .fields
        .title
        .clone()
        .unwrap_or_else(|| "Permission request".into())
}

fn permission_message(tool_call: &ToolCallUpdate) -> String {
    if let Some(content) = &tool_call.fields.content {
        let texts: Vec<&str> = content
            .iter()
            .filter_map(|entry| match entry {
                ToolCallContent::Content(content) => match &content.content {
                    ContentBlock::Text(text) => Some(text.text.as_str()),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        if !texts.is_empty() {
            return texts.join("\n");
        }
    }
    tool_call
        .fields
        .raw_input
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_default()
}

fn emit_debug(events: &UnboundedSender<AgentEvent>, debug_logging: bool, text: impl Into<String>) {
    let text = text.into();
    if debug_logging {
        let _ = events.send(AgentEvent::Debug { text });
    } else {
        log::trace!("{text}");
    }
}

fn build_prompt_blocks(text: String, context: Option<AgentPromptContext>) -> Vec<ContentBlock> {
    let mut blocks = vec![ContentBlock::Text(TextContent::new(text))];
    let Some(context) = context else {
        return blocks;
    };

    let name = context
        .file_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| context.file_path.display().to_string());
    blocks.push(ContentBlock::ResourceLink(ResourceLink::new(
        name,
        path_to_file_uri(&context.file_path),
    )));

    if let Some(selection) = context.selection.filter(|text| !text.is_empty()) {
        blocks.push(ContentBlock::Text(TextContent::new(format!(
            "Selected text from {}:\n```\n{selection}\n```",
            context.file_path.display()
        ))));
    }

    blocks
}

fn path_to_file_uri(path: &std::path::Path) -> String {
    format!("file://{}", path.display())
}

fn handle_cursor_notification(
    events: &UnboundedSender<AgentEvent>,
    debug_logging: bool,
    method: &str,
    params: Value,
) {
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
                    id: request.tool_call_id.clone(),
                    title: request.description,
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
            emit_debug(
                events,
                debug_logging,
                format!("cursor notification ignored: {other}"),
            );
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

fn send_session_update(
    events: &UnboundedSender<AgentEvent>,
    source: &str,
    update: SessionUpdate,
    debug_logging: bool,
) {
    let summary = session_update_summary(&update);
    emit_debug(events, debug_logging, format!("acp[{source}]: {summary}"));
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
        SessionUpdate::ToolCallUpdate(update) => {
            format!("ToolCallUpdate id={}", update.tool_call_id)
        }
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
        SessionUpdate::ToolCall(call) => {
            AgentEvent::Message(map_tool_call_message(&call))
        }
        SessionUpdate::ToolCallUpdate(update) => map_tool_call_update(update),
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

fn format_tool_call_status(status: ToolCallStatus) -> String {
    match status {
        ToolCallStatus::Pending => "pending".into(),
        ToolCallStatus::InProgress => "in_progress".into(),
        ToolCallStatus::Completed => "completed".into(),
        ToolCallStatus::Failed => "failed".into(),
        _ => "unknown".into(),
    }
}

fn format_tool_call_detail(call: &ToolCall) -> Option<String> {
    let mut parts = Vec::new();
    let content = format_tool_call_content(&call.content);
    if !content.is_empty() {
        parts.push(content);
    }
    if let Some(raw_input) = &call.raw_input {
        parts.push(format!("input: {}", pretty_json(raw_input)));
    }
    if let Some(raw_output) = &call.raw_output {
        parts.push(format!("output: {}", pretty_json(raw_output)));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n\n"))
    }
}

fn format_tool_call_content(content: &[ToolCallContent]) -> String {
    content
        .iter()
        .filter_map(|entry| match entry {
            ToolCallContent::Content(content) => match &content.content {
                ContentBlock::Text(text) => Some(text.text.clone()),
                other => Some(content_block_text(other)),
            },
            ToolCallContent::Diff(diff) => Some(format!("diff: {}", diff.path.display())),
            ToolCallContent::Terminal(terminal) => {
                Some(format!("terminal: {}", terminal.terminal_id))
            }
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn pretty_json(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

fn map_tool_call_message(call: &ToolCall) -> AgentMessage {
    AgentMessage::ToolCall {
        id: call.tool_call_id.to_string(),
        title: call.title.clone(),
        status: format_tool_call_status(call.status),
        detail: format_tool_call_detail(call),
    }
}

fn map_tool_call_update(update: ToolCallUpdate) -> AgentEvent {
    let ToolCallUpdate {
        tool_call_id,
        fields:
            ToolCallUpdateFields {
                title,
                status,
                content,
                raw_input,
                raw_output,
                ..
            },
        ..
    } = update;

    let detail = {
        let mut parts = Vec::new();
        if let Some(content) = content {
            let text = format_tool_call_content(&content);
            if !text.is_empty() {
                parts.push(text);
            }
        }
        if let Some(raw_input) = raw_input {
            parts.push(format!("input: {}", pretty_json(&raw_input)));
        }
        if let Some(raw_output) = raw_output {
            parts.push(format!("output: {}", pretty_json(&raw_output)));
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join("\n\n"))
        }
    };

    AgentEvent::ToolCallUpdated {
        id: tool_call_id.to_string(),
        title,
        status: status.map(format_tool_call_status),
        detail,
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
    fn build_prompt_blocks_includes_resource_link_and_selection() {
        let blocks = build_prompt_blocks(
            "fix this".into(),
            Some(AgentPromptContext {
                file_path: PathBuf::from("/tmp/example.rs"),
                selection: Some("fn main() {}".into()),
            }),
        );
        assert_eq!(blocks.len(), 3);
        assert!(matches!(&blocks[0], ContentBlock::Text(_)));
        assert!(matches!(&blocks[1], ContentBlock::ResourceLink(_)));
        assert!(matches!(&blocks[2], ContentBlock::Text(text) if text.text.contains("fn main()")));
    }

    #[test]
    fn map_tool_call_update_emits_update_event() {
        use agent_client_protocol::schema::{
            ToolCallId, ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields,
        };

        let update = ToolCallUpdate::new(
            ToolCallId::new("call-1"),
            ToolCallUpdateFields::new()
                .title("Run tests")
                .status(ToolCallStatus::Completed),
        );
        let event = map_tool_call_update(update);
        assert!(matches!(
            event,
            AgentEvent::ToolCallUpdated {
                id,
                title: Some(title),
                status: Some(status),
                ..
            } if id == "call-1" && title == "Run tests" && status == "completed"
        ));
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
