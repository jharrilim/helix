//! Git panel rendering and input handling.

use std::path::Path;

use crate::compositor::EventResult;
use helix_view::{
    git::GitSelection,
    graphics::{Modifier, Rect},
    input::{MouseButton, MouseEvent, MouseEventKind},
    theme::Style,
    Editor, ViewId,
};
use helix_vcs::{FileChange, StagingSection};
use tui::buffer::Buffer as Surface;
use tui::text::Span;
use tui::widgets::{Block, Borders, Widget};

const ACTION_BAR_HEIGHT: u16 = 1;

#[derive(Clone, Copy, PartialEq, Eq)]
enum GitRow {
    UnstagedHeader,
    StagedHeader,
    File(GitSelection),
}

struct GitLayout {
    rows: Vec<GitRow>,
    list_area: Rect,
}

pub fn panel_at_coords(editor: &Editor, row: u16, column: u16) -> Option<ViewId> {
    editor
        .tree
        .git_panels()
        .find_map(|(panel, _)| contains_coords(panel.area, row, column).then_some(panel.id))
}

fn contains_coords(area: Rect, row: u16, column: u16) -> bool {
    row >= area.y && row < area.bottom() && column >= area.x && column < area.right()
}

fn panel_inner(area: Rect) -> Rect {
    Block::default().borders(Borders::ALL).inner(area)
}

fn action_bar_area(area: Rect) -> Rect {
    let inner = panel_inner(area);
    if inner.height == 0 {
        return Rect::default();
    }
    Rect {
        height: ACTION_BAR_HEIGHT.min(inner.height),
        ..inner
    }
}

fn list_area(area: Rect) -> Rect {
    let inner = panel_inner(area);
    let top = action_bar_area(area).height;
    if inner.height <= top {
        return Rect::default();
    }
    Rect {
        y: inner.y + top,
        height: inner.height.saturating_sub(top),
        ..inner
    }
}

fn build_layout(editor: &Editor, area: Rect) -> GitLayout {
    let list_area = list_area(area);
    let mut rows = Vec::new();

    let unstaged: Vec<_> = editor.git.unstaged().collect();
    let staged: Vec<_> = editor.git.staged().collect();

    if !unstaged.is_empty() {
        rows.push(GitRow::UnstagedHeader);
        for (idx, _) in unstaged.iter().enumerate() {
            rows.push(GitRow::File(GitSelection {
                section: StagingSection::Unstaged,
                index: idx,
            }));
        }
    }
    if !staged.is_empty() {
        rows.push(GitRow::StagedHeader);
        for (idx, _) in staged.iter().enumerate() {
            rows.push(GitRow::File(GitSelection {
                section: StagingSection::Staged,
                index: idx,
            }));
        }
    }

    GitLayout { rows, list_area }
}

fn selection_from_row(row: GitRow) -> Option<GitSelection> {
    match row {
        GitRow::File(selection) => Some(selection),
        _ => None,
    }
}

fn row_at(layout: &GitLayout, scroll: usize, row: u16) -> Option<GitRow> {
    let y = row.saturating_sub(layout.list_area.y) as usize + scroll;
    layout.rows.get(y).copied()
}

pub fn render(editor: &mut Editor, area: Rect, surface: &mut Surface, focused: bool) {
    let border_style = if focused {
        editor.theme.get("ui.border.focused")
    } else {
        editor.theme.get("ui.border")
    };
    let label_style = editor.theme.get("ui.text");
    let branch = editor
        .git
        .branch
        .as_deref()
        .unwrap_or("no branch");
    let title = format!(" Git ({branch}) ");
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Span::styled(title, label_style.add_modifier(Modifier::BOLD)));
    let inner = block.inner(area);
    block.render(area, surface);

    if inner.height == 0 {
        return;
    }

    let action_area = action_bar_area(area);
    let list = list_area(area);
    let button_style = editor.theme.get("ui.text");
    let button_focus = editor.theme.get("ui.selection.active");

    let add_all = "[Add All]";
    let commit = "[Commit]";
    let add_x = action_area.x + 1;
    let commit_x = action_area.right().saturating_sub(commit.len() as u16 + 1);
    editor.git.action_rects.add_all = Rect {
        x: add_x,
        y: action_area.y,
        width: add_all.len() as u16,
        height: 1,
    };
    editor.git.action_rects.commit = Rect {
        x: commit_x,
        y: action_area.y,
        width: commit.len() as u16,
        height: 1,
    };
    surface.set_stringn(
        add_x,
        action_area.y,
        add_all,
        action_area.width as usize,
        button_style.add_modifier(Modifier::BOLD),
    );
    surface.set_stringn(
        commit_x,
        action_area.y,
        commit,
        action_area.width as usize,
        button_focus.add_modifier(Modifier::BOLD),
    );

    let layout = build_layout(editor, area);
    if editor.git.loading {
        surface.set_stringn(
            list.x + 1,
            list.y,
            "Loading...",
            list.width.saturating_sub(2) as usize,
            label_style,
        );
        return;
    }
    if let Some(error) = editor.git.error.as_deref() {
        surface.set_stringn(
            list.x + 1,
            list.y,
            error,
            list.width.saturating_sub(2) as usize,
            editor.theme.get("error"),
        );
        return;
    }
    if layout.rows.is_empty() {
        surface.set_stringn(
            list.x + 1,
            list.y,
            "Working tree clean",
            list.width.saturating_sub(2) as usize,
            label_style,
        );
        return;
    }

    let selection_style = editor.theme.get("ui.selection.active");
    let header_style = label_style.add_modifier(Modifier::BOLD);
    let added = editor.theme.get("diff.plus");
    let modified = editor.theme.get("diff.delta");
    let conflict = editor.theme.get("diff.delta.conflict");
    let deleted = editor.theme.get("diff.minus");
    let renamed = editor.theme.get("diff.delta.moved");
    let cwd = editor.git_cwd();

    let visible_height = list.height as usize;
    let max_scroll = layout.rows.len().saturating_sub(visible_height);
    editor.git.scroll = editor.git.scroll.min(max_scroll);

    for (visible_row, row) in layout
        .rows
        .iter()
        .enumerate()
        .skip(editor.git.scroll)
        .take(visible_height)
    {
        let y = list.y + visible_row as u16;
        let width = list.width.saturating_sub(2) as usize;
        match row {
            GitRow::UnstagedHeader => {
                let count = editor.git.unstaged().count();
                let text = format!("Unstaged ({count})");
                surface.set_stringn(list.x + 1, y, &text, width, header_style);
            }
            GitRow::StagedHeader => {
                let count = editor.git.staged().count();
                let text = format!("Staged ({count})");
                surface.set_stringn(list.x + 1, y, &text, width, header_style);
            }
            GitRow::File(selection) => {
                let Some(entry) = editor
                    .git
                    .entries
                    .iter()
                    .filter(|e| e.section == selection.section)
                    .nth(selection.index)
                else {
                    continue;
                };
                let (label, change_style) =
                    change_label(&entry.change, added, modified, conflict, deleted, renamed);
                let path = display_path(entry.change.path(), &cwd);
                let selected = editor.git.selection == Some(*selection);
                let line = format!(" {label} {path}");
                let style = if selected {
                    selection_style
                } else {
                    change_style
                };
                surface.set_stringn(list.x + 1, y, &line, width, style);
            }
        }
    }
}

fn change_label(
    change: &FileChange,
    added: Style,
    modified: Style,
    conflict: Style,
    deleted: Style,
    renamed: Style,
) -> (&'static str, Style) {
    match change {
        FileChange::Untracked { .. } => ("+", added),
        FileChange::Modified { .. } => ("~", modified),
        FileChange::Conflict { .. } => ("x", conflict),
        FileChange::Deleted { .. } => ("-", deleted),
        FileChange::Renamed { .. } => (">", renamed),
    }
}

fn display_path(path: &Path, cwd: &Path) -> String {
    path.strip_prefix(cwd)
        .unwrap_or(path)
        .display()
        .to_string()
}

pub(crate) fn move_selection_next(editor: &mut Editor) {
    move_selection(editor, 1);
}

pub(crate) fn move_selection_prev(editor: &mut Editor) {
    move_selection(editor, -1);
}

fn move_selection(editor: &mut Editor, delta: i32) {
    let area = editor
        .git
        .panel_id
        .and_then(|id| editor.tree.git_panel(id))
        .map(|panel| panel.area)
        .unwrap_or_default();
    let layout = build_layout(editor, area);
    let file_rows: Vec<GitSelection> = layout
        .rows
        .iter()
        .filter_map(|row| match row {
            GitRow::File(sel) => Some(*sel),
            _ => None,
        })
        .collect();
    if file_rows.is_empty() {
        return;
    }
    let current = editor.git.selection.unwrap_or(file_rows[0]);
    let pos = file_rows.iter().position(|s| *s == current).unwrap_or(0);
    let next = (pos as i32 + delta).clamp(0, file_rows.len() as i32 - 1) as usize;
    editor.git.selection = Some(file_rows[next]);
    ensure_selection_visible(editor, &layout, area);
    helix_event::request_redraw();
}

fn ensure_selection_visible(editor: &mut Editor, layout: &GitLayout, area: Rect) {
    let Some(selection) = editor.git.selection else {
        return;
    };
    let row_index = layout
        .rows
        .iter()
        .position(|row| matches!(row, GitRow::File(s) if *s == selection));
    let Some(row_index) = row_index else {
        return;
    };
    let visible = list_area(area).height as usize;
    if row_index < editor.git.scroll {
        editor.git.scroll = row_index;
    } else if row_index >= editor.git.scroll + visible {
        editor.git.scroll = row_index.saturating_sub(visible.saturating_sub(1));
    }
}

pub fn handle_mouse(editor: &mut Editor, event: MouseEvent) -> EventResult {
    if matches!(event.kind, MouseEventKind::Moved) {
        return EventResult::Ignored(None);
    }

    let panel_id = match panel_at_coords(editor, event.row, event.column) {
        Some(id) => id,
        None if editor.tree.is_git_panel(editor.tree.focus) => editor.tree.focus,
        None => return EventResult::Ignored(None),
    };

    editor.tree.focus = panel_id;
    let Some(panel) = editor.tree.git_panel(panel_id) else {
        return EventResult::Ignored(None);
    };

    let layout = build_layout(editor, panel.area);

    if matches!(event.kind, MouseEventKind::Down(MouseButton::Left)) {
        if contains_coords(editor.git.action_rects.add_all, event.row, event.column) {
            return EventResult::Consumed(Some(Box::new(
                |_compositor, cx| {
                    crate::commands::git::git_stage_all_from_compositor(cx);
                },
            )));
        }
        if contains_coords(editor.git.action_rects.commit, event.row, event.column) {
            return EventResult::Consumed(Some(Box::new(
                |compositor, cx| {
                    crate::commands::git::git_commit_prompt_compositor(compositor, cx.editor);
                },
            )));
        }
        if let Some(row) = row_at(&layout, editor.git.scroll, event.row) {
            if let Some(selection) = selection_from_row(row) {
                editor.git.selection = Some(selection);
            }
        }
        helix_event::request_redraw();
        return EventResult::Consumed(None);
    }

    if matches!(event.kind, MouseEventKind::ScrollDown)
        && contains_coords(layout.list_area, event.row, event.column)
    {
        editor.git.scroll = editor.git.scroll.saturating_add(1);
        helix_event::request_redraw();
        return EventResult::Consumed(None);
    }
    if matches!(event.kind, MouseEventKind::ScrollUp)
        && contains_coords(layout.list_area, event.row, event.column)
    {
        editor.git.scroll = editor.git.scroll.saturating_sub(1);
        helix_event::request_redraw();
        return EventResult::Consumed(None);
    }

    EventResult::Ignored(None)
}
