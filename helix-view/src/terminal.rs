//! Integrated terminal UI state stored on the editor.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::ViewId;

/// Which part of the terminal UI has keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TerminalFocus {
    #[default]
    Normal,
    Insert,
}

/// Working directory policy for new terminal sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TerminalCwd {
    #[default]
    Current,
    WorkspaceRoot,
}

/// Integrated terminal configuration (from user config).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct TerminalSettings {
    /// Enable integrated terminal support.
    pub enable: bool,
    /// Maximum scrollback lines retained per session.
    pub scrollback_lines: usize,
    /// Default cwd for new sessions.
    pub cwd: TerminalCwd,
    /// Focus the terminal panel when opened.
    pub focus_on_open: bool,
    /// Close the panel automatically when the shell exits.
    pub auto_close_on_exit: bool,
    /// Use a dedicated command buffer instead of direct PTY insert mode.
    pub command_buffer: bool,
}

impl Default for TerminalSettings {
    fn default() -> Self {
        Self {
            enable: true,
            scrollback_lines: 10_000,
            cwd: TerminalCwd::Current,
            focus_on_open: true,
            auto_close_on_exit: false,
            command_buffer: false,
        }
    }
}

/// Per-session UI state tracked by the editor.
#[derive(Debug, Clone)]
pub struct TerminalSessionState {
    pub id: String,
    pub panel_id: Option<ViewId>,
    pub title: Option<String>,
    pub cwd: PathBuf,
    pub exit_status: Option<i32>,
    /// When true, scroll position follows new output.
    pub scroll_pinned: bool,
    /// Scrollback offset (mirrors alacritty `display_offset` when unpinned).
    pub scroll_offset: usize,
}

/// Runtime UI state for integrated terminals.
#[derive(Debug, Default)]
pub struct TerminalState {
    pub focus: TerminalFocus,
    pub sessions: HashMap<String, TerminalSessionState>,
    pub active_session: Option<String>,
    next_session_id: usize,
    /// Pending second `g` for scroll-to-top.
    pub pending_scroll_top: bool,
}

impl TerminalState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_open(&self) -> bool {
        self.sessions.values().any(|session| session.panel_id.is_some())
    }

    pub fn register_session(
        &mut self,
        id: impl Into<String>,
        cwd: PathBuf,
    ) -> &mut TerminalSessionState {
        let id = id.into();
        self.sessions.entry(id.clone()).or_insert_with(|| TerminalSessionState {
            id: id.clone(),
            panel_id: None,
            title: None,
            cwd,
            exit_status: None,
            scroll_pinned: true,
            scroll_offset: 0,
        });
        self.active_session = Some(id.clone());
        self.sessions.get_mut(&id).unwrap()
    }

    pub fn next_session_id(&mut self) -> String {
        self.next_session_id += 1;
        format!("terminal-{}", self.next_session_id)
    }

    pub fn remove_session(&mut self, id: &str) {
        self.sessions.remove(id);
        if self.active_session.as_deref() == Some(id) {
            self.active_session = self.sessions.keys().next().cloned();
        }
    }

    pub fn session_for_panel(&self, panel_id: ViewId) -> Option<&TerminalSessionState> {
        self.sessions
            .values()
            .find(|session| session.panel_id == Some(panel_id))
    }

    pub fn session_for_panel_mut(&mut self, panel_id: ViewId) -> Option<&mut TerminalSessionState> {
        self.sessions
            .values_mut()
            .find(|session| session.panel_id == Some(panel_id))
    }

    pub fn session_mut(&mut self, id: &str) -> Option<&mut TerminalSessionState> {
        self.sessions.get_mut(id)
    }

    pub fn active_session_mut(&mut self) -> Option<&mut TerminalSessionState> {
        let id = self.active_session.clone()?;
        self.sessions.get_mut(&id)
    }
}
