//! Integrated terminal UI state stored on the editor.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::ViewId;

/// Which part of the terminal UI has keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TerminalFocus {
    #[default]
    Normal,
    Select,
    Insert,
}

/// Character-wise or line-wise terminal selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TerminalSelectionKind {
    #[default]
    Char,
    Line,
}

/// A point in the terminal grid (alacritty line/column coordinates).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TerminalGridPoint {
    pub line: i32,
    pub col: usize,
}

/// Mouse/keyboard selection within terminal scrollback.
#[derive(Debug, Clone, Copy)]
pub struct TerminalSelection {
    pub anchor: TerminalGridPoint,
    pub head: TerminalGridPoint,
    pub dragging: bool,
    pub kind: TerminalSelectionKind,
}

impl TerminalSelection {
    pub fn new(anchor: TerminalGridPoint, kind: TerminalSelectionKind) -> Self {
        Self {
            anchor,
            head: anchor,
            dragging: false,
            kind,
        }
    }

    pub fn normalized(&self) -> (TerminalGridPoint, TerminalGridPoint) {
        if self.anchor.line < self.head.line
            || (self.anchor.line == self.head.line && self.anchor.col <= self.head.col)
        {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }
}

/// A scrollback search match location.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalSearchMatch {
    pub line: i32,
    pub col: usize,
}

/// Scrollback search state.
#[derive(Debug, Clone)]
pub struct TerminalSearch {
    pub pattern: String,
    pub matches: Vec<TerminalSearchMatch>,
    pub current: usize,
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
    /// Show a status message when the terminal bell rings.
    pub bell: bool,
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
            bell: true,
        }
    }
}

/// Per-session UI state tracked by the editor.
#[derive(Debug, Clone)]
pub struct TerminalSessionState {
    pub id: String,
    pub title: Option<String>,
    pub cwd: PathBuf,
    pub exit_status: Option<i32>,
    /// When true, scroll position follows new output.
    pub scroll_pinned: bool,
    /// Scrollback offset (mirrors alacritty `display_offset` when unpinned).
    pub scroll_offset: usize,
}

impl TerminalSessionState {
    pub fn tab_label(&self) -> &str {
        self.title.as_deref().unwrap_or(&self.id)
    }
}

/// Runtime UI state for integrated terminals.
#[derive(Debug, Default)]
pub struct TerminalState {
    pub focus: TerminalFocus,
    pub sessions: BTreeMap<String, TerminalSessionState>,
    /// Tab order (left to right).
    pub session_order: Vec<String>,
    pub active_session: Option<String>,
    /// Shared terminal panel leaf in the split tree.
    pub panel_id: Option<ViewId>,
    next_session_id: usize,
    /// Pending second `g` for scroll-to-top.
    pub pending_scroll_top: bool,
    /// Active scrollback selection, if any.
    pub selection: Option<TerminalSelection>,
    /// Active scrollback search, if any.
    pub search: Option<TerminalSearch>,
    /// Open a search prompt on the next compositor callback.
    pub open_search_prompt: bool,
    /// Open the tab switcher menu on the next compositor callback.
    pub tab_menu_active: bool,
    /// Waiting for a register name after `"` in Insert mode.
    pub register_pending: bool,
}

impl TerminalState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_open(&self) -> bool {
        self.panel_id.is_some()
    }

    pub fn has_sessions(&self) -> bool {
        !self.sessions.is_empty()
    }

    pub fn insert_session(
        &mut self,
        id: impl Into<String>,
        cwd: PathBuf,
    ) -> &mut TerminalSessionState {
        let id = id.into();
        self.sessions.entry(id.clone()).or_insert_with(|| TerminalSessionState {
            id: id.clone(),
            title: None,
            cwd,
            exit_status: None,
            scroll_pinned: true,
            scroll_offset: 0,
        });
        if !self.session_order.contains(&id) {
            self.session_order.push(id.clone());
        }
        self.active_session = Some(id.clone());
        self.sessions.get_mut(&id).unwrap()
    }

    /// Backwards-compatible alias for callers that used `register_session`.
    pub fn register_session(
        &mut self,
        id: impl Into<String>,
        cwd: PathBuf,
    ) -> &mut TerminalSessionState {
        self.insert_session(id, cwd)
    }

    pub fn next_session_id(&mut self) -> String {
        self.next_session_id += 1;
        format!("terminal-{}", self.next_session_id)
    }

    pub fn active_index(&self) -> Option<usize> {
        let active = self.active_session.as_ref()?;
        self.session_order.iter().position(|id| id == active)
    }

    pub fn switch_session(&mut self, id: &str) -> bool {
        if !self.sessions.contains_key(id) {
            return false;
        }
        self.active_session = Some(id.to_string());
        self.clear_selection();
        self.clear_search();
        if self.focus == TerminalFocus::Insert {
            self.focus = TerminalFocus::Normal;
        }
        true
    }

    pub fn switch_relative(&mut self, delta: isize) -> bool {
        if self.session_order.is_empty() {
            return false;
        }
        let current = self.active_index().unwrap_or(0);
        let len = self.session_order.len();
        let next = (current as isize + delta).rem_euclid(len as isize) as usize;
        let id = self.session_order[next].clone();
        self.switch_session(&id)
    }

    /// Remove a session tab. Returns true if the panel should be removed (last tab).
    pub fn remove_session(&mut self, id: &str) -> bool {
        self.sessions.remove(id);
        self.session_order.retain(|session_id| session_id != id);
        if self.active_session.as_deref() == Some(id) {
            self.active_session = self
                .session_order
                .last()
                .or_else(|| self.session_order.first())
                .cloned();
        }
        self.session_order.is_empty()
    }

    pub fn ordered_sessions(&self) -> impl Iterator<Item = &TerminalSessionState> {
        self.session_order
            .iter()
            .filter_map(|id| self.sessions.get(id))
    }

    pub fn session_mut(&mut self, id: &str) -> Option<&mut TerminalSessionState> {
        self.sessions.get_mut(id)
    }

    pub fn active_session_mut(&mut self) -> Option<&mut TerminalSessionState> {
        let id = self.active_session.clone()?;
        self.sessions.get_mut(&id)
    }

    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    pub fn clear_search(&mut self) {
        self.search = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_order_tracks_insert_and_remove() {
        let mut state = TerminalState::new();
        state.insert_session("terminal-1", PathBuf::from("/a"));
        state.insert_session("terminal-2", PathBuf::from("/b"));
        assert_eq!(
            state.session_order,
            vec!["terminal-1".to_string(), "terminal-2".to_string()]
        );
        assert!(!state.remove_session("terminal-1"));
        assert_eq!(state.active_session.as_deref(), Some("terminal-2"));
        assert!(state.remove_session("terminal-2"));
        assert!(state.session_order.is_empty());
    }

    #[test]
    fn switch_relative_wraps() {
        let mut state = TerminalState::new();
        state.insert_session("terminal-1", PathBuf::from("/a"));
        state.insert_session("terminal-2", PathBuf::from("/b"));
        state.switch_session("terminal-1");
        state.switch_relative(1);
        assert_eq!(state.active_session.as_deref(), Some("terminal-2"));
        state.switch_relative(1);
        assert_eq!(state.active_session.as_deref(), Some("terminal-1"));
    }
}
