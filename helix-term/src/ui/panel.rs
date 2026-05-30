//! Unified dispatch for auxiliary panel leaves in the split tree.

use crate::compositor::EventResult;
use crate::commands;
use helix_core::Position;
use helix_view::{
    graphics::{CursorKind, Rect},
    input::{KeyEvent, MouseButton, MouseEvent, MouseEventKind},
    tree::LeafKind,
    Editor, ViewId,
};
use tui::buffer::Buffer as Surface;

use super::{agent, git, plan, terminal, EditorView};

const PANEL_HIT_ORDER: [LeafKind; 5] = [
    LeafKind::GitPanel,
    LeafKind::ReviewPanel,
    LeafKind::PlanPanel,
    LeafKind::AgentPanel,
    LeafKind::TerminalPanel,
];

pub fn panel_at_coords(editor: &Editor, row: u16, column: u16) -> Option<(LeafKind, ViewId)> {
    for kind in PANEL_HIT_ORDER {
        if let Some(id) = panel_at_coords_kind(editor, kind, row, column) {
            return Some((kind, id));
        }
    }
    None
}

fn panel_at_coords_kind(
    editor: &Editor,
    kind: LeafKind,
    row: u16,
    column: u16,
) -> Option<ViewId> {
    match kind {
        LeafKind::GitPanel => git::panel_at_coords(editor, row, column),
        LeafKind::ReviewPanel => crate::ui::review_panel::panel_at_coords(editor, row, column),
        LeafKind::PlanPanel => plan::panel_at_coords(editor, row, column),
        LeafKind::AgentPanel => agent::panel_at_coords(editor, row, column),
        LeafKind::TerminalPanel => terminal::panel_at_coords(editor, row, column),
        LeafKind::View => None,
    }
}

pub fn handle_mouse(editor: &mut Editor, kind: LeafKind, event: MouseEvent) -> EventResult {
    match kind {
        LeafKind::GitPanel => git::handle_mouse(editor, event),
        LeafKind::ReviewPanel => crate::ui::review_panel::handle_mouse(editor, event),
        LeafKind::PlanPanel => plan::handle_mouse(editor, event),
        LeafKind::AgentPanel => agent::handle_mouse(editor, event),
        LeafKind::TerminalPanel => terminal::handle_mouse(editor, event),
        LeafKind::View => EventResult::Ignored(None),
    }
}

pub fn handle_insert_key(editor: &mut Editor, kind: LeafKind, key: KeyEvent) -> bool {
    match kind {
        LeafKind::AgentPanel => agent::handle_key(editor, key),
        LeafKind::TerminalPanel => terminal::handle_key(editor, key),
        LeafKind::GitPanel | LeafKind::ReviewPanel | LeafKind::PlanPanel | LeafKind::View => false,
    }
}

pub fn handle_normal_key(
    view: &mut EditorView,
    cx: &mut commands::Context,
    kind: LeafKind,
    key: KeyEvent,
) -> bool {
    match kind {
        LeafKind::GitPanel => view.handle_git_normal_key(cx, key),
        LeafKind::ReviewPanel => view.handle_review_panel_normal_key(cx, key),
        LeafKind::PlanPanel => view.handle_plan_normal_key(cx, key),
        LeafKind::AgentPanel => view.handle_agent_normal_key(cx, key),
        LeafKind::TerminalPanel => view.handle_terminal_normal_key(cx, key),
        LeafKind::View => false,
    }
}

pub fn render(
    editor: &mut Editor,
    kind: LeafKind,
    area: Rect,
    surface: &mut Surface,
    focused: bool,
) {
    match kind {
        LeafKind::GitPanel => git::render(editor, area, surface, focused),
        LeafKind::ReviewPanel => crate::ui::review_panel::render(editor, area, surface, focused),
        LeafKind::PlanPanel => plan::render(editor, area, surface, focused),
        LeafKind::AgentPanel => agent::render(editor, area, surface, focused),
        LeafKind::TerminalPanel => terminal::render(editor, area, surface, focused),
        LeafKind::View => {}
    }
}

pub fn cursor(
    editor: &Editor,
    kind: LeafKind,
    area: Rect,
    input_focused: bool,
) -> (Option<Position>, CursorKind) {
    match kind {
        LeafKind::GitPanel | LeafKind::ReviewPanel | LeafKind::PlanPanel => {
            (None, CursorKind::Hidden)
        }
        LeafKind::AgentPanel if input_focused => agent::cursor(editor, area),
        LeafKind::TerminalPanel if input_focused => terminal::cursor(editor, area),
        LeafKind::AgentPanel | LeafKind::TerminalPanel => (None, CursorKind::Hidden),
        LeafKind::View => editor.cursor(),
    }
}

pub fn panel_input_focused(editor: &Editor, kind: LeafKind) -> bool {
    match kind {
        LeafKind::AgentPanel => editor.agent_input_focused(),
        LeafKind::TerminalPanel => editor.terminal_input_focused(),
        LeafKind::GitPanel | LeafKind::ReviewPanel | LeafKind::PlanPanel | LeafKind::View => false,
    }
}

pub fn is_auxiliary_panel(kind: &LeafKind) -> bool {
    !matches!(kind, LeafKind::View)
}

pub fn panel_wants_off_area_event(editor: &Editor, kind: LeafKind, event: &MouseEvent) -> bool {
    match kind {
        LeafKind::AgentPanel => {
            editor.tree.is_agent_panel(editor.tree.focus)
                && matches!(
                    event.kind,
                    MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Up(MouseButton::Left)
                )
                && editor
                    .agent
                    .transcript_selection
                    .as_ref()
                    .is_some_and(|sel| sel.dragging || matches!(event.kind, MouseEventKind::Up(_)))
        }
        LeafKind::TerminalPanel => {
            editor.tree.is_terminal_panel(editor.tree.focus)
                && matches!(
                    event.kind,
                    MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Up(MouseButton::Left)
                )
                && editor
                    .terminal
                    .selection
                    .as_ref()
                    .is_some_and(|sel| sel.dragging || matches!(event.kind, MouseEventKind::Up(_)))
        }
        LeafKind::GitPanel | LeafKind::ReviewPanel | LeafKind::PlanPanel | LeafKind::View => false,
    }
}
