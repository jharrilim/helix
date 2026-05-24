//! Agent panel UI state stored on the editor.

use std::collections::VecDeque;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Which part of the agent UI has keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentFocus {
    #[default]
    Normal,
    Insert,
}

/// A position within the wrapped transcript layout (line and character column).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AgentTranscriptPoint {
    pub line: usize,
    pub col: usize,
}

/// Mouse selection within the agent transcript.
#[derive(Debug, Clone, Copy, Default)]
pub struct AgentTranscriptSelection {
    pub anchor: AgentTranscriptPoint,
    pub head: AgentTranscriptPoint,
    pub dragging: bool,
}

impl AgentTranscriptSelection {
    pub fn new(anchor: AgentTranscriptPoint) -> Self {
        Self {
            anchor,
            head: anchor,
            dragging: true,
        }
    }

    pub fn normalized(&self) -> (AgentTranscriptPoint, AgentTranscriptPoint) {
        if self.anchor.line < self.head.line
            || (self.anchor.line == self.head.line && self.anchor.col <= self.head.col)
        {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }
}

/// A transcript entry displayed in the agent panel.
#[derive(Debug, Clone)]
pub enum AgentTranscriptEntry {
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
        expanded: bool,
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

impl AgentTranscriptEntry {
    pub fn append_chunk(&mut self, chunk: &str) {
        match self {
            Self::User { text }
            | Self::Assistant { text }
            | Self::Thought { text }
            | Self::System { text }
            | Self::Error { text } => text.push_str(chunk),
            _ => {}
        }
    }

    pub fn role_label(&self) -> &'static str {
        match self {
            Self::User { .. } => "user",
            Self::Assistant { .. } => "assistant",
            Self::Thought { .. } => "thought",
            Self::ToolCall { .. } => "tool",
            Self::Plan { .. } => "plan",
            Self::System { .. } => "system",
            Self::Error { .. } => "error",
        }
    }
}

/// Session metadata tracked by the editor UI.
#[derive(Debug, Clone)]
pub struct AgentSessionMeta {
    pub id: String,
    pub title: Option<String>,
    pub cwd: PathBuf,
    pub updated_at: Option<String>,
}

/// Agent panel configuration (from user config).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "kebab-case")]
pub struct AgentSettings {
    /// Enable agent integration.
    pub enable: bool,
    /// Shell command to spawn the ACP agent.
    pub command: String,
    /// Agent panel width as percent of editor width (1-50).
    pub panel_width_percent: u8,
    /// Preferred ACP auth method id (e.g. `cursor_login`).
    pub auth_method: Option<String>,
    /// Skip the authenticate RPC even when the agent advertises auth methods.
    pub skip_authenticate: bool,
    /// Default session mode applied after session/new or session/load.
    pub default_mode: Option<String>,
    /// Optional override path to an MCP config file.
    pub mcp_config_path: Option<PathBuf>,
    /// Emit verbose ACP debug lines in the agent pane.
    pub debug_logging: bool,
    /// Auto-approve permission requests without showing a picker.
    pub auto_approve_permissions: bool,
    /// Attach current buffer path and selection to outgoing prompts.
    pub include_editor_context: bool,
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            enable: true,
            command: String::new(),
            panel_width_percent: 35,
            auth_method: None,
            skip_authenticate: false,
            default_mode: None,
            mcp_config_path: None,
            debug_logging: false,
            auto_approve_permissions: false,
            include_editor_context: true,
        }
    }
}

impl AgentSettings {
    pub fn panel_width_fraction(&self) -> f32 {
        (self.panel_width_percent.clamp(10, 50) as f32) / 100.0
    }
}

/// Session mode metadata from the agent.
#[derive(Debug, Clone)]
pub struct AgentModeMeta {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}

/// Pending Cursor extension request shown in the UI.
#[derive(Debug, Clone)]
pub struct AgentCursorRequest {
    pub request_id: u64,
    pub method: String,
    pub params: serde_json::Value,
}

/// Option for a Cursor ask_question prompt.
#[derive(Debug, Clone)]
pub struct AgentQuestionOption {
    pub id: String,
    pub label: String,
}

/// Question in a Cursor ask_question flow.
#[derive(Debug, Clone)]
pub struct AgentQuestion {
    pub id: String,
    pub prompt: String,
    pub options: Vec<AgentQuestionOption>,
    pub allow_multiple: bool,
}

/// Answer collected for a Cursor ask_question flow.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentQuestionAnswer {
    pub question_id: String,
    pub selected_option_ids: Vec<String>,
}

/// In-progress multi-question Cursor ask_question UI state.
#[derive(Debug, Clone)]
pub struct AgentQuestionFlow {
    pub request_id: u64,
    pub title: Option<String>,
    pub questions: Vec<AgentQuestion>,
    pub answers: Vec<AgentQuestionAnswer>,
    pub index: usize,
}

/// Runtime UI state for the agent panel.
#[derive(Debug, Default)]
pub struct AgentState {
    pub focus: AgentFocus,
    /// Tree node id of the agent panel, if open.
    pub panel_id: Option<crate::ViewId>,
    pub active_session: Option<String>,
    pub sessions: Vec<AgentSessionMeta>,
    pub transcript: Vec<AgentTranscriptEntry>,
    pub input: String,
    pub input_cursor: usize,
    pub scroll: usize,
    pub pending: bool,
    pub status: Option<String>,
    pub error: Option<String>,
    pub prompt_history: VecDeque<String>,
    pub history_pos: Option<usize>,
    pub transcript_selection: Option<AgentTranscriptSelection>,
    /// When true, the next `SessionList` event should open the history picker.
    pub open_session_picker: bool,
    /// Recent agent/ACP debug lines shown in the transcript pane.
    pub debug_log: VecDeque<String>,
    /// Messages received while replaying a loaded session (reset on load start).
    pub load_replay_count: u32,
    /// Current agent session mode id, if known.
    pub mode: Option<String>,
    /// Available session modes reported by the agent.
    pub available_modes: Vec<AgentModeMeta>,
    /// Blocking Cursor extension request awaiting UI response.
    pub cursor_request: Option<AgentCursorRequest>,
    /// When true, show the Cursor extension UI on the next event dispatch.
    pub open_cursor_request: bool,
    /// In-progress Cursor ask_question flow.
    pub cursor_question_flow: Option<AgentQuestionFlow>,
    /// Blocking permission request awaiting UI response.
    pub pending_permission: Option<AgentPendingPermission>,
    /// When true, show the permission picker on the next event dispatch.
    pub open_permission_picker: bool,
    /// Whether the agent supports ACP session/close.
    pub close_session: bool,
    /// When true, filter session history picker to the current working directory.
    pub history_filter_cwd: bool,
    /// Index into `transcript` of the focused tool row for keyboard toggle.
    pub transcript_tool_focus: Option<usize>,
}

/// Option for an ACP permission request.
#[derive(Debug, Clone)]
pub struct AgentPermissionOption {
    pub id: String,
    pub label: String,
}

/// Pending permission request shown in the UI.
#[derive(Debug, Clone)]
pub struct AgentPendingPermission {
    pub request_id: u64,
    pub title: String,
    pub message: String,
    pub options: Vec<AgentPermissionOption>,
}

impl AgentState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_open(&self) -> bool {
        self.panel_id.is_some()
    }

    pub fn push_entry(&mut self, entry: AgentTranscriptEntry) {
        self.transcript.push(entry);
    }

    pub fn append_or_push(&mut self, entry: AgentTranscriptEntry) {
        let same_role = self
            .transcript
            .last()
            .map(|last| last.role_label() == entry.role_label())
            .unwrap_or(false);

        if same_role {
            if let Some(last) = self.transcript.last_mut() {
                if let AgentTranscriptEntry::User { text }
                | AgentTranscriptEntry::Assistant { text }
                | AgentTranscriptEntry::Thought { text } = entry
                {
                    last.append_chunk(&text);
                    if self.scroll == 0 {
                        self.scroll = 0;
                    }
                    return;
                }
            }
        }
        self.push_entry(entry);
    }

    pub fn clear_transcript(&mut self) {
        self.transcript.clear();
        self.scroll = 0;
        self.transcript_selection = None;
        self.transcript_tool_focus = None;
    }

    pub fn upsert_tool_call(
        &mut self,
        id: String,
        title: String,
        status: String,
        detail: Option<String>,
    ) {
        if let Some(index) = self
            .transcript
            .iter()
            .position(|entry| matches!(entry, AgentTranscriptEntry::ToolCall { id: existing, .. } if existing == &id))
        {
            if let AgentTranscriptEntry::ToolCall {
                title: existing_title,
                status: existing_status,
                detail: existing_detail,
                expanded,
                ..
            } = &mut self.transcript[index]
            {
                *existing_title = title;
                let collapsed = tool_status_collapsed(&status);
                *existing_status = status;
                if detail.is_some() {
                    *existing_detail = detail;
                }
                if collapsed {
                    *expanded = false;
                }
            }
            return;
        }

        let expanded = !tool_status_collapsed(&status);
        self.push_entry(AgentTranscriptEntry::ToolCall {
            id,
            title,
            status,
            detail,
            expanded,
        });
    }

    pub fn update_tool_call(
        &mut self,
        id: &str,
        title: Option<String>,
        status: Option<String>,
        detail: Option<String>,
    ) {
        let Some(index) = self.transcript.iter().position(|entry| {
            matches!(entry, AgentTranscriptEntry::ToolCall { id: existing, .. } if existing == id)
        }) else {
            return;
        };
        if let AgentTranscriptEntry::ToolCall {
            title: existing_title,
            status: existing_status,
            detail: existing_detail,
            expanded,
            ..
        } = &mut self.transcript[index]
        {
            if let Some(title) = title {
                *existing_title = title;
            }
            if let Some(status) = status {
                *existing_status = status.clone();
                if tool_status_collapsed(&status) {
                    *expanded = false;
                }
            }
            if detail.is_some() {
                *existing_detail = detail;
            }
        }
    }

    pub fn toggle_tool_call(&mut self, index: usize) {
        if let Some(AgentTranscriptEntry::ToolCall { expanded, .. }) =
            self.transcript.get_mut(index)
        {
            *expanded = !*expanded;
        }
    }

    pub fn tool_call_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.transcript
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                matches!(entry, AgentTranscriptEntry::ToolCall { .. }).then_some(index)
            })
    }

    pub fn clear_transcript_selection(&mut self) {
        self.transcript_selection = None;
    }

    pub fn set_sessions(&mut self, sessions: Vec<AgentSessionMeta>) {
        self.sessions = sessions;
    }

    pub fn push_debug(&mut self, line: impl Into<String>) {
        const MAX_DEBUG_LINES: usize = 20;
        self.debug_log.push_back(line.into());
        while self.debug_log.len() > MAX_DEBUG_LINES {
            self.debug_log.pop_front();
        }
    }

    pub fn clear_debug(&mut self) {
        self.debug_log.clear();
        self.load_replay_count = 0;
    }
}

fn tool_status_collapsed(status: &str) -> bool {
    matches!(status, "completed" | "failed")
}
