//! ACP agent runtime integration and buffer-aware filesystem handlers.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::Arc;

use helix_acp::{
    AgentCommand, AgentConfig, AgentEvent, AgentMessage, AgentRuntime, AgentRuntimeHandle,
    FsReadFn, FsReadResult, FsWriteFn, FsWriteRequest, FsWriteResult, TerminalCreateFn,
    TerminalCreateRequest, TerminalExitResult, TerminalKillFn, TerminalOutputFn,
    TerminalOutputSnapshot, TerminalReleaseFn, TerminalWaitExitFn, truncate_output,
};
use helix_core::{diff, Rope};
use helix_pty::{TerminalCommand, TerminalId};
use helix_view::{
    agent::{
        AgentBlockKind, AgentCursorRequest, AgentModeMeta, AgentPermissionOption,
        AgentPendingPermission, AgentSessionMeta,
    },
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

static NEXT_AGENT_TERMINAL_ID: AtomicU64 = AtomicU64::new(0);

/// Bridges ACP terminal callbacks to the main thread and PTY runtime.
pub struct TerminalBridge {
    create_requests: mpsc::Receiver<(TerminalCreateRequest, SyncSender<Result<String, String>>)>,
    output_requests: mpsc::Receiver<(String, SyncSender<TerminalOutputSnapshot>)>,
    wait_requests: mpsc::Receiver<(String, SyncSender<Result<TerminalExitResult, String>>)>,
    kill_requests: mpsc::Receiver<(String, SyncSender<Result<(), String>>)>,
    release_requests: mpsc::Receiver<(String, SyncSender<Result<(), String>>)>,
    exit_results: HashMap<String, TerminalExitResult>,
    exit_waiters: HashMap<String, Vec<SyncSender<Result<TerminalExitResult, String>>>>,
}

impl TerminalBridge {
    fn new() -> (
        Self,
        TerminalCreateFn,
        TerminalOutputFn,
        TerminalWaitExitFn,
        TerminalKillFn,
        TerminalReleaseFn,
    ) {
        let (create_tx, create_rx) = mpsc::channel();
        let (output_tx, output_rx) = mpsc::channel();
        let (wait_tx, wait_rx) = mpsc::channel();
        let (kill_tx, kill_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();

        let create_fn: TerminalCreateFn = Arc::new({
            let create_tx = create_tx.clone();
            move |request| {
                let (resp_tx, resp_rx) = mpsc::sync_channel(1);
                if create_tx.send((request, resp_tx)).is_err() {
                    return Err("agent terminal bridge closed".into());
                }
                helix_event::request_redraw();
                resp_rx
                    .recv()
                    .unwrap_or_else(|_| Err("agent terminal bridge closed".into()))
            }
        });

        let output_fn: TerminalOutputFn = Arc::new({
            let output_tx = output_tx.clone();
            move |terminal_id| {
                let (resp_tx, resp_rx) = mpsc::sync_channel(1);
                if output_tx.send((terminal_id, resp_tx)).is_err() {
                    return TerminalOutputSnapshot {
                        output: String::new(),
                        truncated: false,
                        exit_code: None,
                    };
                }
                helix_event::request_redraw();
                resp_rx.recv().unwrap_or(TerminalOutputSnapshot {
                    output: String::new(),
                    truncated: false,
                    exit_code: None,
                })
            }
        });

        let wait_fn: TerminalWaitExitFn = Arc::new({
            let wait_tx = wait_tx.clone();
            move |terminal_id| {
                let (resp_tx, resp_rx) = mpsc::sync_channel(1);
                if wait_tx.send((terminal_id, resp_tx)).is_err() {
                    return Err("agent terminal bridge closed".into());
                }
                helix_event::request_redraw();
                resp_rx
                    .recv()
                    .unwrap_or_else(|_| Err("agent terminal bridge closed".into()))
            }
        });

        let kill_fn: TerminalKillFn = Arc::new({
            let kill_tx = kill_tx.clone();
            move |terminal_id| {
                let (resp_tx, resp_rx) = mpsc::sync_channel(1);
                if kill_tx.send((terminal_id, resp_tx)).is_err() {
                    return Err("agent terminal bridge closed".into());
                }
                helix_event::request_redraw();
                resp_rx
                    .recv()
                    .unwrap_or_else(|_| Err("agent terminal bridge closed".into()))
            }
        });

        let release_fn: TerminalReleaseFn = Arc::new({
            let release_tx = release_tx.clone();
            move |terminal_id| {
                let (resp_tx, resp_rx) = mpsc::sync_channel(1);
                if release_tx.send((terminal_id, resp_tx)).is_err() {
                    return Err("agent terminal bridge closed".into());
                }
                helix_event::request_redraw();
                resp_rx
                    .recv()
                    .unwrap_or_else(|_| Err("agent terminal bridge closed".into()))
            }
        });

        (
            Self {
                create_requests: create_rx,
                output_requests: output_rx,
                wait_requests: wait_rx,
                kill_requests: kill_rx,
                release_requests: release_rx,
                exit_results: HashMap::new(),
                exit_waiters: HashMap::new(),
            },
            create_fn,
            output_fn,
            wait_fn,
            kill_fn,
            release_fn,
        )
    }

    fn poll(&mut self, editor: &mut Editor) {
        while let Ok((request, resp)) = self.create_requests.try_recv() {
            let _ = resp.send(terminal_create_impl(editor, request));
        }
        while let Ok((terminal_id, resp)) = self.output_requests.try_recv() {
            let _ = resp.send(terminal_output_impl(editor, &self.exit_results, &terminal_id));
        }
        while let Ok((terminal_id, resp)) = self.wait_requests.try_recv() {
            terminal_wait_impl(self, &terminal_id, resp);
        }
        while let Ok((terminal_id, resp)) = self.kill_requests.try_recv() {
            let _ = resp.send(terminal_kill_impl(&terminal_id));
        }
        while let Ok((terminal_id, resp)) = self.release_requests.try_recv() {
            let _ = resp.send(terminal_release_impl(editor, &terminal_id));
        }
    }

    fn store_exit(&mut self, terminal_id: &str, result: TerminalExitResult) {
        self.exit_results.insert(terminal_id.to_string(), result.clone());
        if let Some(waiters) = self.exit_waiters.remove(terminal_id) {
            for waiter in waiters {
                let _ = waiter.send(Ok(result.clone()));
            }
        }
    }
}

fn next_agent_terminal_id() -> String {
    let id = NEXT_AGENT_TERMINAL_ID.fetch_add(1, Ordering::Relaxed);
    format!("agent-term-{id}")
}

fn terminal_create_impl(
    editor: &mut Editor,
    request: TerminalCreateRequest,
) -> Result<String, String> {
    let terminal_id = next_agent_terminal_id();
    spawn_shell_program(
        editor,
        &terminal_id,
        request.command,
        request.args,
        request.env,
        request.cwd,
        request.output_byte_limit,
    );
    Ok(terminal_id)
}

fn spawn_shell_program(
    editor: &mut Editor,
    terminal_id: &str,
    command: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
    cwd: Option<PathBuf>,
    output_byte_limit: Option<u64>,
) {
    editor.agent.upsert_shell_block(
        terminal_id.to_string(),
        command.clone(),
        args.clone(),
        output_byte_limit,
    );
    crate::terminal::send(TerminalCommand::SpawnProgram {
        id: TerminalId(terminal_id.to_string()),
        command,
        args,
        env,
        cwd,
        rows: 24,
        cols: 80,
    });
}

fn try_run_tool_shell_command(editor: &mut Editor, tool_id: &str, command: &str) {
    if editor.agent.tool_shell_index.contains_key(tool_id) {
        return;
    }
    let terminal_id = format!("tool-{tool_id}");
    editor.agent.link_tool_to_terminal(tool_id, &terminal_id);
    let cwd = editor
        .last_cwd
        .clone()
        .or_else(|| std::env::current_dir().ok());
    crate::terminal::send(TerminalCommand::SpawnProgram {
        id: TerminalId(terminal_id),
        command: "/bin/sh".into(),
        args: vec!["-c".into(), command.into()],
        env: Vec::new(),
        cwd,
        rows: 24,
        cols: 80,
    });
}

fn link_tool_to_existing_terminal(editor: &mut Editor, tool_id: &str, terminal_id: &str) {
    if editor.agent.tool_shell_index.contains_key(tool_id) {
        return;
    }
    editor.agent.link_tool_to_terminal(tool_id, terminal_id);
    if let Some(handle) = crate::terminal::session_handle(terminal_id) {
        let output = handle.plain_text_output();
        editor.agent.set_shell_output(terminal_id, output);
    }
}

fn handle_tool_shell(
    editor: &mut Editor,
    tool_id: &str,
    shell_command: Option<String>,
    terminal_id: Option<String>,
) {
    if let Some(terminal_id) = terminal_id {
        link_tool_to_existing_terminal(editor, tool_id, &terminal_id);
    }
    if let Some(command) = shell_command.filter(|command| !command.is_empty()) {
        try_run_tool_shell_command(editor, tool_id, &command);
    }
}

fn terminal_output_impl(
    editor: &Editor,
    exit_results: &HashMap<String, TerminalExitResult>,
    terminal_id: &str,
) -> TerminalOutputSnapshot {
    let output = crate::terminal::session_handle(terminal_id)
        .map(|handle| handle.plain_text_output())
        .unwrap_or_default();
    let byte_limit = editor
        .agent
        .shell_output_limits
        .get(terminal_id)
        .copied()
        .unwrap_or(u64::MAX);
    let (output, truncated) = truncate_output(&output, byte_limit);
    let exit_code = exit_results
        .get(terminal_id)
        .and_then(|result| result.exit_code);
    TerminalOutputSnapshot {
        output,
        truncated,
        exit_code,
    }
}

fn terminal_wait_impl(
    bridge: &mut TerminalBridge,
    terminal_id: &str,
    resp: SyncSender<Result<TerminalExitResult, String>>,
) {
    if let Some(result) = bridge.exit_results.get(terminal_id) {
        let _ = resp.send(Ok(result.clone()));
        return;
    }
    bridge
        .exit_waiters
        .entry(terminal_id.to_string())
        .or_default()
        .push(resp);
}

fn terminal_kill_impl(terminal_id: &str) -> Result<(), String> {
    crate::terminal::send(TerminalCommand::Kill {
        id: TerminalId(terminal_id.to_string()),
    });
    Ok(())
}

fn terminal_release_impl(editor: &mut Editor, terminal_id: &str) -> Result<(), String> {
    crate::terminal::send(TerminalCommand::Kill {
        id: TerminalId(terminal_id.to_string()),
    });
    editor.agent.remove_shell_terminal(terminal_id);
    Ok(())
}

/// Called when a PTY session linked to an agent shell block exits.
pub fn notify_agent_terminal_exited(
    editor: &mut Editor,
    bridge: &mut TerminalBridge,
    terminal_id: &str,
    code: Option<i32>,
    signal: Option<i32>,
) {
    let failed = signal.is_some() || code.is_some_and(|code| code != 0);
    if editor.agent.shell_block_index.contains_key(terminal_id) {
        editor.agent.finish_shell_block(terminal_id, code, failed);
    }
    if let Some(tool_id) = editor
        .agent
        .tool_id_for_terminal(terminal_id)
        .map(str::to_string)
    {
        editor.agent.finish_tool_shell(&tool_id, code, failed);
    }
    bridge.store_exit(
        terminal_id,
        TerminalExitResult {
            exit_code: code,
            signal,
        },
    );
}

/// Called when PTY output updates for an agent shell block.
pub fn notify_agent_terminal_updated(editor: &mut Editor, terminal_id: &str) {
    let Some(handle) = crate::terminal::session_handle(terminal_id) else {
        return;
    };
    let output = handle.plain_text_output();
    if editor.agent.shell_block_index.contains_key(terminal_id) {
        editor.agent.set_shell_output(terminal_id, output);
    } else if let Some(tool_id) = editor
        .agent
        .tool_id_for_terminal(terminal_id)
        .map(str::to_string)
    {
        editor.agent.set_tool_shell_output(&tool_id, output);
    }
}

#[derive(Default)]
pub struct AgentController {
    runtime: Option<AgentRuntimeHandle>,
    events_rx: Option<UnboundedReceiver<AgentEvent>>,
    fs_bridge: Option<FsBridge>,
    terminal_bridge: Option<TerminalBridge>,
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
        let (terminal_bridge, terminal_create, terminal_output, terminal_wait, terminal_kill, terminal_release) =
            TerminalBridge::new();
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

        let (handle, events_rx) = AgentRuntime::spawn(
            config,
            fs_read,
            fs_write,
            terminal_create,
            terminal_output,
            terminal_wait,
            terminal_kill,
            terminal_release,
        );
        self.runtime = Some(handle);
        self.events_rx = Some(events_rx);
        self.fs_bridge = Some(fs_bridge);
        self.terminal_bridge = Some(terminal_bridge);
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
        self.terminal_bridge = None;
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
        if let Some(bridge) = self.terminal_bridge.as_mut() {
            bridge.poll(editor);
        }
    }

    pub fn on_terminal_updated(&mut self, editor: &mut Editor, terminal_id: &str) {
        notify_agent_terminal_updated(editor, terminal_id);
    }

    pub fn on_terminal_exited(
        &mut self,
        editor: &mut Editor,
        terminal_id: &str,
        code: Option<i32>,
        signal: Option<i32>,
    ) {
        if let Some(bridge) = self.terminal_bridge.as_mut() {
            notify_agent_terminal_exited(editor, bridge, terminal_id, code, signal);
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
    if !controller.is_running() {
        editor.set_error("agent is not running (use :agent-open)");
        return;
    }
    if editor.agent.active_session.is_none() {
        ensure_session(controller, editor);
        editor.set_status("starting agent session...");
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
                editor.agent.blocks.len()
            ));
            if editor.agent.blocks.is_empty() {
                editor.agent.push_block(AgentBlockKind::System {
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
                    shell_command,
                    terminal_id,
                } => {
                    editor.agent.upsert_tool_call(
                        id.clone(),
                        title.clone(),
                        status.clone(),
                        detail.clone(),
                        terminal_id.clone(),
                    );
                    handle_tool_shell(
                        editor,
                        &id,
                        shell_command.clone(),
                        terminal_id.clone(),
                    );
                }
                _ => {
                    if let Some(kind) = map_message(msg.clone()) {
                        editor.agent.push_message_block(kind);
                    }
                }
            }
        }
        AgentEvent::ToolCallUpdated {
            id,
            title,
            status,
            detail,
            shell_command,
            terminal_id,
            agent_output,
        } => {
            editor.agent.update_tool_call(
                id,
                title.clone(),
                status.clone(),
                detail.clone(),
                terminal_id.clone(),
                agent_output.clone(),
            );
            handle_tool_shell(editor, id, shell_command.clone(), terminal_id.clone());
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
                .push_block(AgentBlockKind::Error { text: text.clone() });
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

fn map_message(msg: AgentMessage) -> Option<AgentBlockKind> {
    Some(match msg {
        AgentMessage::User { text } if !text.is_empty() => AgentBlockKind::User { text },
        AgentMessage::Assistant { text } if !text.is_empty() => {
            AgentBlockKind::Assistant { text }
        }
        AgentMessage::Thought { text } if !text.is_empty() => AgentBlockKind::Thought { text },
        AgentMessage::ToolCall { .. } => return None,
        AgentMessage::Plan { entries } => AgentBlockKind::Plan { entries },
        AgentMessage::System { text } if !text.is_empty() => AgentBlockKind::System { text },
        AgentMessage::Error { text } if !text.is_empty() => AgentBlockKind::Error { text },
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
    use helix_view::agent::AgentBlockKind;

    #[test]
    fn map_message_covers_user_role() {
        let kind = map_message(AgentMessage::User {
            text: "hello".into(),
        })
        .unwrap();
        assert!(matches!(kind, AgentBlockKind::User { .. }));
    }
}
