use std::path::PathBuf;

use crate::session::{AgentSessionId, AgentSessionInfo};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A single item in the agent session transcript.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentMessage {
    User {
        text: String,
    },
    Assistant {
        text: String,
    },
    Thought {
        text: String,
    },
    ToolCall {
        id: String,
        title: String,
        status: String,
        detail: Option<String>,
    },
    Plan {
        entries: Vec<String>,
    },
    System {
        text: String,
    },
    Error {
        text: String,
    },
}

/// Session mode metadata from the agent.
#[derive(Debug, Clone)]
pub struct AgentModeInfo {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}

/// Editor buffer context attached to outgoing prompts.
#[derive(Debug, Clone)]
pub struct AgentPromptContext {
    pub file_path: PathBuf,
    pub selection: Option<String>,
}

/// Permission option for an interactive permission request.
#[derive(Debug, Clone)]
pub struct AgentPermissionOption {
    pub id: String,
    pub label: String,
}

/// Events emitted by the ACP runtime to the UI layer.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    Initialized {
        agent_name: Option<String>,
        agent_version: Option<String>,
        auth_methods: Vec<String>,
        load_session: bool,
        close_session: bool,
    },
    SessionClosed,
    Authenticated {
        method_id: String,
    },
    SessionStarted {
        session_id: AgentSessionId,
    },
    SessionLoadStarted {
        session_id: AgentSessionId,
    },
    SessionLoaded {
        session_id: AgentSessionId,
    },
    SessionList {
        sessions: Vec<AgentSessionInfo>,
    },
    SessionInfoUpdated {
        title: Option<Option<String>>,
        updated_at: Option<Option<String>>,
    },
    ModeUpdated {
        current_mode: String,
        available_modes: Vec<AgentModeInfo>,
    },
    Message(AgentMessage),
    ToolCallUpdated {
        id: String,
        title: Option<String>,
        status: Option<String>,
        detail: Option<String>,
    },
    TurnStarted,
    TurnFinished {
        stop_reason: Option<String>,
    },
    TurnCancelled,
    Status {
        text: String,
    },
    Error {
        text: String,
    },
    PermissionRequested {
        request_id: u64,
        title: String,
        message: String,
        options: Vec<AgentPermissionOption>,
    },
    /// Blocking Cursor extension request that requires UI interaction.
    CursorRequest {
        request_id: u64,
        method: String,
        params: Value,
    },
    /// Diagnostic line for agent panel debugging (session load/replay).
    Debug {
        text: String,
    },
}

/// Commands sent from the UI to the ACP runtime.
#[derive(Debug)]
pub enum AgentCommand {
    StartSession {
        cwd: Option<PathBuf>,
    },
    LoadSession {
        session_id: AgentSessionId,
        cwd: Option<PathBuf>,
    },
    ListSessions {
        cwd: Option<PathBuf>,
    },
    SendPrompt {
        text: String,
        context: Option<AgentPromptContext>,
    },
    Cancel,
    CloseSession,
    NewSession {
        cwd: Option<PathBuf>,
    },
    Stop,
    SetMode {
        mode_id: String,
    },
    RespondPermission {
        request_id: u64,
        option_id: Option<String>,
    },
    RespondCursor {
        request_id: u64,
        result: Value,
    },
}
