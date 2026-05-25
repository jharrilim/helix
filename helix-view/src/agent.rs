//! Agent panel UI state stored on the editor.

use std::collections::{HashMap, HashSet, VecDeque};
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

/// Lifecycle status for a shell block backed by an ACP terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShellBlockStatus {
    #[default]
    Running,
    Completed,
    Failed,
}

/// Kind of content in an agent transcript block.
#[derive(Debug, Clone)]
pub enum AgentBlockKind {
    User {
        text: String,
    },
    Assistant {
        text: String,
    },
    Thought {
        text: String,
    },
    Tool {
        id: String,
        title: String,
        status: String,
        detail: Option<String>,
        /// Live PTY output when this tool runs a shell command locally.
        shell_output: String,
        /// Terminal session id when linked to a shell block or tool PTY.
        linked_terminal_id: Option<String>,
        expanded: bool,
    },
    Shell {
        terminal_id: String,
        command: String,
        args: Vec<String>,
        output: String,
        status: ShellBlockStatus,
        exit_code: Option<i32>,
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

impl AgentBlockKind {
    pub fn role_label(&self) -> &'static str {
        match self {
            Self::User { .. } => "user",
            Self::Assistant { .. } => "assistant",
            Self::Thought { .. } => "thought",
            Self::Tool { .. } => "tool",
            Self::Shell { .. } => "shell",
            Self::Plan { .. } => "plan",
            Self::System { .. } => "system",
            Self::Error { .. } => "error",
        }
    }

    pub fn is_collapsible(&self) -> bool {
        matches!(self, Self::Tool { .. } | Self::Shell { .. })
    }
}

/// A single block in the agent transcript.
#[derive(Debug, Clone)]
pub struct AgentBlock {
    pub id: String,
    pub kind: AgentBlockKind,
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
    pub blocks: Vec<AgentBlock>,
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
    /// Index into `blocks` of the focused collapsible row for keyboard toggle.
    pub block_collapsible_focus: Option<usize>,
    /// Maps ACP terminal ids to block indices for shell output streaming.
    pub shell_block_index: HashMap<String, usize>,
    /// Maps tool call ids to terminal ids for shell command tools.
    pub tool_shell_index: HashMap<String, String>,
    /// Tool calls blocked from showing output or running locally until approved.
    pub permission_gated_tools: HashSet<String>,
    /// Tool calls the user has approved this session.
    pub granted_tool_permissions: HashSet<String>,
    /// Shell/output updates held until permission is granted.
    pub deferred_tool_shell: HashMap<String, DeferredToolShell>,
    /// Monotonic counter for generating block ids.
    next_block_id: u64,
    /// Per-terminal output byte limits from ACP `terminal/create`.
    pub shell_output_limits: HashMap<String, u64>,
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
    pub tool_call_id: Option<String>,
    pub title: String,
    pub message: String,
    pub options: Vec<AgentPermissionOption>,
}

/// Shell execution deferred until the user approves a tool permission request.
#[derive(Debug, Clone, Default)]
pub struct DeferredToolShell {
    pub shell_command: Option<String>,
    pub terminal_id: Option<String>,
    pub agent_output: Option<String>,
}

impl AgentState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_open(&self) -> bool {
        self.panel_id.is_some()
    }

    fn next_block_id(&mut self) -> String {
        self.next_block_id += 1;
        format!("block-{}", self.next_block_id)
    }

    pub fn push_block(&mut self, kind: AgentBlockKind) {
        let id = self.next_block_id();
        self.blocks.push(AgentBlock { id, kind });
    }

    /// Appends streaming text chunks to the last block when the role matches.
    pub fn push_message_block(&mut self, kind: AgentBlockKind) {
        let chunk = match &kind {
            AgentBlockKind::User { text }
            | AgentBlockKind::Assistant { text }
            | AgentBlockKind::Thought { text } => text.as_str(),
            _ => {
                self.push_block(kind);
                return;
            }
        };

        if let Some(last) = self.blocks.last_mut() {
            let same_role = matches!(
                (&last.kind, &kind),
                (AgentBlockKind::User { .. }, AgentBlockKind::User { .. })
                    | (AgentBlockKind::Assistant { .. }, AgentBlockKind::Assistant { .. })
                    | (AgentBlockKind::Thought { .. }, AgentBlockKind::Thought { .. })
            );
            if same_role {
                match &mut last.kind {
                    AgentBlockKind::User { text }
                    | AgentBlockKind::Assistant { text }
                    | AgentBlockKind::Thought { text } => {
                        text.push_str(chunk);
                        return;
                    }
                    _ => {}
                }
            }
        }

        self.push_block(kind);
    }

    pub fn clear_transcript(&mut self) {
        self.blocks.clear();
        self.scroll = 0;
        self.transcript_selection = None;
        self.block_collapsible_focus = None;
        self.shell_block_index.clear();
        self.shell_output_limits.clear();
        self.tool_shell_index.clear();
        self.permission_gated_tools.clear();
        self.granted_tool_permissions.clear();
        self.deferred_tool_shell.clear();
    }

    pub fn tool_requires_permission(&self, tool_call_id: &str) -> bool {
        self.permission_gated_tools.contains(tool_call_id)
            && !self.granted_tool_permissions.contains(tool_call_id)
    }

    pub fn gate_tool_for_permission(&mut self, tool_call_id: &str) {
        if self.granted_tool_permissions.contains(tool_call_id) {
            return;
        }
        self.permission_gated_tools
            .insert(tool_call_id.to_string());
        self.withhold_tool_output(tool_call_id);
        if let Some(index) = self.blocks.iter().position(|block| {
            matches!(&block.kind, AgentBlockKind::Tool { id, .. } if id == tool_call_id)
        }) {
            if let AgentBlockKind::Tool { status, expanded, .. } = &mut self.blocks[index].kind {
                if !self.granted_tool_permissions.contains(tool_call_id) {
                    *status = "awaiting permission".into();
                }
                *expanded = true;
            }
        }
    }

    pub fn grant_tool_permission(&mut self, tool_call_id: &str) {
        self.granted_tool_permissions
            .insert(tool_call_id.to_string());
        self.permission_gated_tools.remove(tool_call_id);
    }

    fn withhold_tool_output(&mut self, tool_call_id: &str) {
        let Some(index) = self.blocks.iter().position(|block| {
            matches!(&block.kind, AgentBlockKind::Tool { id, .. } if id == tool_call_id)
        }) else {
            return;
        };
        let AgentBlockKind::Tool {
            detail,
            shell_output,
            ..
        } = &mut self.blocks[index].kind
        else {
            return;
        };

        let mut withheld = std::mem::take(shell_output);
        if let Some(detail_text) = detail.take() {
            if !detail_text.is_empty() {
                if !withheld.is_empty() {
                    withheld.push_str("\n\n");
                }
                withheld.push_str(&detail_text);
            }
        }
        if withheld.is_empty() {
            return;
        }

        let entry = self
            .deferred_tool_shell
            .entry(tool_call_id.to_string())
            .or_default();
        match &mut entry.agent_output {
            Some(existing) => {
                if !existing.contains(withheld.as_str()) {
                    existing.push_str("\n\n");
                    existing.push_str(&withheld);
                }
            }
            None => entry.agent_output = Some(withheld),
        }
    }

    pub fn tool_permission_pending(&self, tool_call_id: &str) -> bool {
        self.tool_requires_permission(tool_call_id)
    }

    pub fn defer_tool_shell(
        &mut self,
        tool_call_id: &str,
        shell_command: Option<String>,
        terminal_id: Option<String>,
        agent_output: Option<String>,
    ) {
        let entry = self
            .deferred_tool_shell
            .entry(tool_call_id.to_string())
            .or_default();
        if shell_command.is_some() {
            entry.shell_command = shell_command;
        }
        if terminal_id.is_some() {
            entry.terminal_id = terminal_id;
        }
        if agent_output.is_some() {
            entry.agent_output = agent_output;
        }
    }

    pub fn take_deferred_tool_shell(&mut self, tool_call_id: &str) -> Option<DeferredToolShell> {
        self.deferred_tool_shell.remove(tool_call_id)
    }

    pub fn clear_tool_permission_state(&mut self, tool_call_id: &str) {
        self.permission_gated_tools.remove(tool_call_id);
        self.deferred_tool_shell.remove(tool_call_id);
    }

    pub fn upsert_tool_call(
        &mut self,
        id: String,
        title: String,
        status: String,
        detail: Option<String>,
        linked_terminal_id: Option<String>,
    ) {
        if let Some(index) = self
            .blocks
            .iter()
            .position(|block| matches!(&block.kind, AgentBlockKind::Tool { id: existing, .. } if existing == &id))
        {
            if let AgentBlockKind::Tool {
                title: existing_title,
                status: existing_status,
                detail: existing_detail,
                linked_terminal_id: existing_terminal,
                expanded,
                ..
            } = &mut self.blocks[index].kind
            {
                *existing_title = title;
                let collapsed = tool_status_collapsed(&status);
                *existing_status = status;
                merge_tool_detail(existing_detail, detail);
                if linked_terminal_id.is_some() {
                    *existing_terminal = linked_terminal_id;
                }
                if collapsed {
                    *expanded = false;
                }
            }
            return;
        }

        let expanded = !tool_status_collapsed(&status);
        self.push_block(AgentBlockKind::Tool {
            id,
            title,
            status,
            detail,
            shell_output: String::new(),
            linked_terminal_id,
            expanded,
        });
    }

    pub fn update_tool_call(
        &mut self,
        id: &str,
        title: Option<String>,
        status: Option<String>,
        detail: Option<String>,
        linked_terminal_id: Option<String>,
        agent_output: Option<String>,
    ) {
        let Some(index) = self.blocks.iter().position(|block| {
            matches!(&block.kind, AgentBlockKind::Tool { id: existing, .. } if existing == id)
        }) else {
            return;
        };
        if let AgentBlockKind::Tool {
            title: existing_title,
            status: existing_status,
            detail: existing_detail,
            shell_output,
            linked_terminal_id: existing_terminal,
            expanded,
            ..
        } = &mut self.blocks[index].kind
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
            merge_tool_detail(existing_detail, detail);
            if let Some(output) = agent_output {
                if !output.is_empty() {
                    *shell_output = output;
                }
            }
            if linked_terminal_id.is_some() {
                *existing_terminal = linked_terminal_id;
            }
        }
    }

    pub fn link_tool_to_terminal(&mut self, tool_id: &str, terminal_id: &str) {
        self.tool_shell_index
            .insert(tool_id.to_string(), terminal_id.to_string());
        let Some(index) = self.blocks.iter().position(|block| {
            matches!(&block.kind, AgentBlockKind::Tool { id, .. } if id == tool_id)
        }) else {
            return;
        };
        if let AgentBlockKind::Tool {
            linked_terminal_id,
            expanded,
            ..
        } = &mut self.blocks[index].kind
        {
            *linked_terminal_id = Some(terminal_id.to_string());
            *expanded = true;
        }
    }

    pub fn set_tool_shell_output(&mut self, tool_id: &str, output: String) {
        if self.tool_requires_permission(tool_id) {
            self.defer_tool_shell(tool_id, None, None, Some(output));
            return;
        }
        let Some(index) = self.blocks.iter().position(|block| {
            matches!(&block.kind, AgentBlockKind::Tool { id, .. } if id == tool_id)
        }) else {
            return;
        };
        if let AgentBlockKind::Tool { shell_output, expanded, .. } =
            &mut self.blocks[index].kind
        {
            *shell_output = output;
            *expanded = true;
        }
    }

    pub fn finish_tool_shell(&mut self, tool_id: &str, exit_code: Option<i32>, failed: bool) {
        let Some(index) = self.blocks.iter().position(|block| {
            matches!(&block.kind, AgentBlockKind::Tool { id, .. } if id == tool_id)
        }) else {
            return;
        };
        if let AgentBlockKind::Tool {
            status,
            expanded,
            ..
        } = &mut self.blocks[index].kind
        {
            *status = if failed {
                exit_code
                    .map(|code| format!("failed ({code})"))
                    .unwrap_or_else(|| "failed".into())
            } else {
                exit_code
                    .map(|code| format!("completed ({code})"))
                    .unwrap_or_else(|| "completed".into())
            };
            if !failed {
                *expanded = false;
            }
        }
    }

    pub fn tool_id_for_terminal(&self, terminal_id: &str) -> Option<&str> {
        self.tool_shell_index
            .iter()
            .find_map(|(tool_id, linked)| (linked == terminal_id).then_some(tool_id.as_str()))
    }

    pub fn upsert_shell_block(
        &mut self,
        terminal_id: String,
        command: String,
        args: Vec<String>,
        output_byte_limit: Option<u64>,
    ) {
        if let Some(&index) = self.shell_block_index.get(&terminal_id) {
            if let AgentBlockKind::Shell {
                command: existing_command,
                args: existing_args,
                ..
            } = &mut self.blocks[index].kind
            {
                *existing_command = command;
                *existing_args = args;
            }
            if let Some(limit) = output_byte_limit {
                self.shell_output_limits.insert(terminal_id.clone(), limit);
            }
            return;
        }

        if let Some(limit) = output_byte_limit {
            self.shell_output_limits.insert(terminal_id.clone(), limit);
        }

        let index = self.blocks.len();
        self.push_block(AgentBlockKind::Shell {
            terminal_id: terminal_id.clone(),
            command,
            args,
            output: String::new(),
            status: ShellBlockStatus::Running,
            exit_code: None,
            expanded: true,
        });
        self.shell_block_index.insert(terminal_id, index);
    }

    pub fn set_shell_output(&mut self, terminal_id: &str, output: String) {
        if let Some(&index) = self.shell_block_index.get(terminal_id) {
            if let AgentBlockKind::Shell {
                output: existing,
                ..
            } = &mut self.blocks[index].kind
            {
                *existing = output.clone();
            }
        }
        if let Some(tool_id) = self.tool_id_for_terminal(terminal_id).map(str::to_string) {
            self.set_tool_shell_output(&tool_id, output);
        }
    }

    pub fn finish_shell_block(&mut self, terminal_id: &str, exit_code: Option<i32>, failed: bool) {
        let Some(&index) = self.shell_block_index.get(terminal_id) else {
            return;
        };
        if let AgentBlockKind::Shell {
            status,
            exit_code: existing_code,
            expanded,
            ..
        } = &mut self.blocks[index].kind
        {
            *status = if failed {
                ShellBlockStatus::Failed
            } else {
                ShellBlockStatus::Completed
            };
            *existing_code = exit_code;
            *expanded = false;
        }
    }

    pub fn toggle_collapsible_block(&mut self, index: usize) {
        match self.blocks.get_mut(index).map(|block| &mut block.kind) {
            Some(AgentBlockKind::Tool { expanded, .. }) => *expanded = !*expanded,
            Some(AgentBlockKind::Shell { expanded, .. }) => *expanded = !*expanded,
            _ => {}
        }
    }

    pub fn collapsible_block_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.blocks.iter().enumerate().filter_map(|(index, block)| {
            block.kind.is_collapsible().then_some(index)
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

    pub fn remove_shell_terminal(&mut self, terminal_id: &str) {
        self.shell_block_index.remove(terminal_id);
        self.shell_output_limits.remove(terminal_id);
    }
}

fn tool_status_collapsed(status: &str) -> bool {
    matches!(status, "completed" | "failed") || status.starts_with("completed")
        || status.starts_with("failed")
}

fn merge_tool_detail(existing: &mut Option<String>, incoming: Option<String>) {
    let Some(incoming) = incoming else {
        return;
    };
    if incoming.is_empty() {
        return;
    }
    match existing {
        None => *existing = Some(incoming),
        Some(existing) => {
            if existing.contains(incoming.as_str()) {
                return;
            }
            if incoming.starts_with("output:") && !existing.contains("output:") {
                existing.push_str("\n\n");
                existing.push_str(&incoming);
                return;
            }
            if incoming.len() > existing.len() {
                *existing = incoming;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_message_block_merges_consecutive_assistant_chunks() {
        let mut state = AgentState::new();
        state.push_message_block(AgentBlockKind::Assistant {
            text: "hello ".into(),
        });
        state.push_message_block(AgentBlockKind::Assistant {
            text: "world".into(),
        });
        assert_eq!(state.blocks.len(), 1);
        let AgentBlockKind::Assistant { text } = &state.blocks[0].kind else {
            panic!("expected assistant block");
        };
        assert_eq!(text, "hello world");
    }

    #[test]
    fn push_message_block_does_not_merge_different_roles() {
        let mut state = AgentState::new();
        state.push_message_block(AgentBlockKind::User {
            text: "hi".into(),
        });
        state.push_message_block(AgentBlockKind::Assistant {
            text: "hello".into(),
        });
        state.push_message_block(AgentBlockKind::Assistant {
            text: "!".into(),
        });
        assert_eq!(state.blocks.len(), 2);
    }

    #[test]
    fn shell_output_updates_by_terminal_id() {
        let mut state = AgentState::new();
        state.upsert_shell_block(
            "term-1".into(),
            "echo".into(),
            vec!["hi".into()],
            None,
        );
        state.set_shell_output("term-1", "hi\n".into());
        let AgentBlockKind::Shell { output, .. } = &state.blocks[0].kind else {
            panic!("expected shell block");
        };
        assert_eq!(output, "hi\n");
    }
}
