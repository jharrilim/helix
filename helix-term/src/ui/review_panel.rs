//! Code review sidebar panel rendering and input.

use crate::compositor::EventResult;
use helix_review::ReviewStatus;
use helix_view::{
    graphics::{Modifier, Rect},
    input::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind},
    review_panel::ReviewPanelSelection,
    Editor, ViewId,
};
use tui::buffer::Buffer as Surface;
use tui::text::Span;
use tui::widgets::{Block, Widget};

const ACTION_BAR_HEIGHT: u16 = 1;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReviewRow {
    ReviewsHeader,
    Review(usize),
    CommentsHeader,
    Comment(usize),
}

struct ReviewLayout {
    rows: Vec<ReviewRow>,
    list_area: Rect,
}

pub fn panel_at_coords(editor: &Editor, row: u16, column: u16) -> Option<ViewId> {
    editor.tree.review_panels().find_map(|(panel, _)| {
        contains_coords(panel.area, row, column).then_some(panel.id)
    })
}

fn contains_coords(area: Rect, row: u16, column: u16) -> bool {
    row >= area.y && row < area.bottom() && column >= area.x && column < area.right()
}

fn panel_inner(area: Rect) -> Rect {
    super::panel_style::panel_inner(area)
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

pub fn refresh_list(editor: &mut Editor) {
    editor.review.ensure_repo_slug();
    let repo_slug = editor.review.repo_slug.clone();
    editor.review_panel.entries = helix_view::list_reviews(&repo_slug);
    if editor.review_panel.selection.is_none() {
        if !editor.review_panel.entries.is_empty() {
            editor.review_panel.selection = Some(ReviewPanelSelection::Review(0));
        } else if editor
            .review
            .current
            .as_ref()
            .is_some_and(|r| !r.comments.is_empty())
        {
            editor.review_panel.selection = Some(ReviewPanelSelection::Comment(0));
        }
    }
}

fn build_layout(editor: &Editor, area: Rect) -> ReviewLayout {
    let list_area = list_area(area);
    let mut rows = Vec::new();

    rows.push(ReviewRow::ReviewsHeader);
    for index in 0..editor.review_panel.entries.len() {
        rows.push(ReviewRow::Review(index));
    }

    if let Some(review) = editor.review.current.as_ref() {
        if !review.comments.is_empty() {
            rows.push(ReviewRow::CommentsHeader);
            for index in 0..review.comments.len() {
                rows.push(ReviewRow::Comment(index));
            }
        }
    }

    ReviewLayout { rows, list_area }
}

fn selection_from_row(row: ReviewRow) -> Option<ReviewPanelSelection> {
    match row {
        ReviewRow::Review(index) => Some(ReviewPanelSelection::Review(index)),
        ReviewRow::Comment(index) => Some(ReviewPanelSelection::Comment(index)),
        _ => None,
    }
}

fn row_at(layout: &ReviewLayout, scroll: usize, row: u16) -> Option<ReviewRow> {
    let y = row.saturating_sub(layout.list_area.y) as usize + scroll;
    layout.rows.get(y).copied()
}

fn status_label(status: ReviewStatus) -> &'static str {
    match status {
        ReviewStatus::Draft => "draft",
        ReviewStatus::Submitted => "submitted",
    }
}

fn truncate_middle(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let keep = max.saturating_sub(1) / 2;
    let start: String = text.chars().take(keep).collect();
    let end: String = text
        .chars()
        .rev()
        .take(keep)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("{start}…{end}")
}

fn comment_line(editor: &Editor, index: usize, width: usize) -> String {
    let Some(review) = editor.review.current.as_ref() else {
        return String::new();
    };
    let Some(comment) = review.comments.get(index) else {
        return String::new();
    };
    let repo_root = review.metadata.repo_root.as_path();
    let path = comment
        .file
        .strip_prefix(repo_root)
        .unwrap_or(comment.file.as_path());
    let location = format!("{}:{}", path.display(), comment.line + 1);
    let body = truncate_middle(comment.body.lines().next().unwrap_or(""), 24);
    format!(" {location} — {body}")
        .chars()
        .take(width)
        .collect()
}

pub fn render(editor: &mut Editor, area: Rect, surface: &mut Surface, _focused: bool) {
    let border_style = super::panel_style::border_style(&editor.theme);
    let label_style = editor.theme.get("ui.text");
    let title = if editor.review.active {
        " Code Review "
    } else {
        " Code Review (off) "
    };
    let block = Block::default()
        .borders(super::panel_style::panel_borders())
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

    let new_label = "[(N)ew]";
    let submit_label = "[(S)ubmit]";
    let delete_label = "[(D)elete]";
    let new_x = action_area.x + 1;
    let submit_x = new_x + new_label.len() as u16 + 1;
    let delete_x = action_area.right().saturating_sub(delete_label.len() as u16 + 1);

    editor.review_panel.action_rects.new_review = Rect {
        x: new_x,
        y: action_area.y,
        width: new_label.len() as u16,
        height: 1,
    };
    editor.review_panel.action_rects.submit = Rect {
        x: submit_x,
        y: action_area.y,
        width: submit_label.len() as u16,
        height: 1,
    };
    editor.review_panel.action_rects.delete = Rect {
        x: delete_x,
        y: action_area.y,
        width: delete_label.len() as u16,
        height: 1,
    };

    surface.set_stringn(
        new_x,
        action_area.y,
        new_label,
        action_area.width as usize,
        button_style.add_modifier(Modifier::BOLD),
    );
    surface.set_stringn(
        submit_x,
        action_area.y,
        submit_label,
        action_area.width as usize,
        button_focus.add_modifier(Modifier::BOLD),
    );
    surface.set_stringn(
        delete_x,
        action_area.y,
        delete_label,
        action_area.width as usize,
        button_style.add_modifier(Modifier::BOLD),
    );

    let layout = build_layout(editor, area);
    if layout.rows.is_empty() {
        surface.set_stringn(
            list.x + 1,
            list.y,
            "No reviews yet — press New",
            list.width.saturating_sub(2) as usize,
            label_style,
        );
        return;
    }

    let selection_style = editor.theme.get("ui.selection.active");
    let header_style = label_style.add_modifier(Modifier::BOLD);
    let dim_style = label_style.add_modifier(Modifier::DIM);
    let current_id = editor.review.current.as_ref().map(|r| r.metadata.id.as_str());

    let visible_height = list.height as usize;
    let max_scroll = layout.rows.len().saturating_sub(visible_height);
    editor.review_panel.scroll = editor.review_panel.scroll.min(max_scroll);

    for (visible_row, row) in layout
        .rows
        .iter()
        .enumerate()
        .skip(editor.review_panel.scroll)
        .take(visible_height)
    {
        let y = list.y + visible_row as u16;
        let width = list.width.saturating_sub(2) as usize;
        let selected = layout
            .rows
            .iter()
            .enumerate()
            .skip(editor.review_panel.scroll)
            .nth(visible_row)
            .and_then(|(_, r)| selection_from_row(*r))
            == editor.review_panel.selection;

        match row {
            ReviewRow::ReviewsHeader => {
                let count = editor.review_panel.entries.len();
                let text = format!("Reviews ({count})");
                surface.set_stringn(list.x + 1, y, &text, width, header_style);
            }
            ReviewRow::Review(index) => {
                let Some(entry) = editor.review_panel.entries.get(*index) else {
                    continue;
                };
                let active = current_id == Some(entry.id.as_str());
                let marker = if active { "› " } else { "  " };
                let text = format!(
                    "{marker}{}{} ({}) · {}",
                    entry.title,
                    if active { " *" } else { "" },
                    entry.comment_count,
                    status_label(entry.status)
                );
                let style = if selected {
                    selection_style
                } else if active {
                    button_focus
                } else {
                    label_style
                };
                surface.set_stringn(list.x + 1, y, &text, width, style);
            }
            ReviewRow::CommentsHeader => {
                let count = editor.review.current.as_ref().map(|r| r.comments.len()).unwrap_or(0);
                let text = format!("Comments ({count})");
                surface.set_stringn(list.x + 1, y, &text, width, header_style);
            }
            ReviewRow::Comment(index) => {
                let line = comment_line(editor, *index, width.saturating_sub(2));
                let style = if selected { selection_style } else { dim_style };
                surface.set_stringn(list.x + 1, y, &line, width, style);
            }
        }
    }
}

pub fn move_selection_next(editor: &mut Editor) {
    move_selection(editor, 1);
}

pub fn move_selection_prev(editor: &mut Editor) {
    move_selection(editor, -1);
}

fn selectable_rows(layout: &ReviewLayout) -> Vec<ReviewPanelSelection> {
    layout
        .rows
        .iter()
        .filter_map(|row| selection_from_row(*row))
        .collect()
}

fn move_selection(editor: &mut Editor, delta: i32) {
    let area = editor
        .review_panel
        .panel_id
        .and_then(|id| editor.tree.review_panel(id))
        .map(|panel| panel.area)
        .unwrap_or_default();
    let layout = build_layout(editor, area);
    let rows = selectable_rows(&layout);
    if rows.is_empty() {
        return;
    }
    let current = editor.review_panel.selection.unwrap_or(rows[0]);
    let pos = rows.iter().position(|s| *s == current).unwrap_or(0);
    let next = (pos as i32 + delta).clamp(0, rows.len() as i32 - 1) as usize;
    editor.review_panel.selection = Some(rows[next]);
    ensure_selection_visible(editor, &layout, area);
    helix_event::request_redraw();
}

fn ensure_selection_visible(editor: &mut Editor, layout: &ReviewLayout, area: Rect) {
    let Some(selection) = editor.review_panel.selection else {
        return;
    };
    let row_index = layout.rows.iter().position(|row| {
        selection_from_row(*row)
            .is_some_and(|candidate| candidate == selection)
    });
    let Some(row_index) = row_index else {
        return;
    };
    let visible = list_area(area).height as usize;
    if row_index < editor.review_panel.scroll {
        editor.review_panel.scroll = row_index;
    } else if row_index >= editor.review_panel.scroll + visible {
        editor.review_panel.scroll = row_index.saturating_sub(visible.saturating_sub(1));
    }
}

pub fn handle_normal_key(editor: &mut Editor, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            move_selection_next(editor);
            true
        }
        KeyCode::Char('k') | KeyCode::Up => {
            move_selection_prev(editor);
            true
        }
        KeyCode::Enter => {
            crate::commands::review::review_panel_activate_editor(editor);
            true
        }
        KeyCode::Char('d') if key.modifiers.is_empty() => {
            crate::commands::review::review_panel_delete_editor(editor);
            true
        }
        KeyCode::Char('n') if key.modifiers.is_empty() => {
            crate::commands::review::review_new_editor(editor);
            refresh_list(editor);
            helix_event::request_redraw();
            true
        }
        KeyCode::Char('s') if key.modifiers.is_empty() => {
            editor.review.active = true;
            helix_event::request_redraw();
            true
        }
        KeyCode::Char('r') if key.modifiers.is_empty() => {
            refresh_list(editor);
            helix_event::request_redraw();
            true
        }
        _ => false,
    }
}

pub fn handle_mouse(editor: &mut Editor, event: MouseEvent) -> EventResult {
    if matches!(event.kind, MouseEventKind::Moved) {
        return EventResult::Ignored(None);
    }

    let panel_id = match panel_at_coords(editor, event.row, event.column) {
        Some(id) => id,
        None => return EventResult::Ignored(None),
    };

    editor.tree.focus = panel_id;
    let Some(panel) = editor.tree.review_panel(panel_id) else {
        return EventResult::Ignored(None);
    };

    let layout = build_layout(editor, panel.area);

    if matches!(event.kind, MouseEventKind::Down(MouseButton::Left)) {
        if contains_coords(editor.review_panel.action_rects.new_review, event.row, event.column) {
            crate::commands::review::review_new_editor(editor);
            refresh_list(editor);
            helix_event::request_redraw();
            return EventResult::Consumed(None);
        }
        if contains_coords(editor.review_panel.action_rects.submit, event.row, event.column) {
            return EventResult::Consumed(Some(Box::new(|_compositor, cx| {
                crate::commands::review::review_submit_editor(cx.editor, cx.jobs);
                crate::ui::review_panel::refresh_list(cx.editor);
                helix_event::request_redraw();
            })));
        }
        if contains_coords(editor.review_panel.action_rects.delete, event.row, event.column) {
            crate::commands::review::review_panel_delete_editor(editor);
            return EventResult::Consumed(None);
        }
        if let Some(row) = row_at(&layout, editor.review_panel.scroll, event.row) {
            if let Some(selection) = selection_from_row(row) {
                editor.review_panel.selection = Some(selection);
            }
        }
        helix_event::request_redraw();
        return EventResult::Consumed(None);
    }

    if matches!(event.kind, MouseEventKind::ScrollDown)
        && contains_coords(layout.list_area, event.row, event.column)
    {
        editor.review_panel.scroll = editor.review_panel.scroll.saturating_add(1);
        helix_event::request_redraw();
        return EventResult::Consumed(None);
    }
    if matches!(event.kind, MouseEventKind::ScrollUp)
        && contains_coords(layout.list_area, event.row, event.column)
    {
        editor.review_panel.scroll = editor.review_panel.scroll.saturating_sub(1);
        helix_event::request_redraw();
        return EventResult::Consumed(None);
    }

    EventResult::Ignored(None)
}
