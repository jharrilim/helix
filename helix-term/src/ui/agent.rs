//! Agent panel rendering and input handling.

use crate::compositor::EventResult;
use crate::ui::Markdown;
use helix_core::unicode::segmentation::UnicodeSegmentation;
use helix_core::unicode::width::UnicodeWidthStr;
use helix_core::Position;
use helix_view::{
    agent::{AgentBlockKind, AgentFocus, AgentTranscriptPoint, AgentTranscriptSelection, ShellBlockStatus},
    graphics::{CursorKind, Modifier, Rect},
    input::{KeyEvent, MouseButton, MouseEvent, MouseEventKind},
    keyboard::{KeyCode, KeyModifiers},
    theme::Style,
    Editor, ViewId,
};
use tui::buffer::Buffer as Surface;
use tui::text::{Span, Spans};
use tui::widgets::{Block, Borders, Widget};

const INPUT_HEIGHT: u16 = 3;
const HEADER_HEIGHT: u16 = 1;

#[derive(Clone)]
struct TranscriptLine {
    text: String,
    spans: Vec<TranscriptSpan>,
    kind: TranscriptLineKind,
    entry_index: Option<usize>,
}

#[derive(Clone)]
struct TranscriptSpan {
    text: String,
    style: Style,
}

#[derive(Clone, Copy)]
enum TranscriptLineKind {
    Role,
    Body { thought: bool, error: bool },
    ToolHeader,
    ToolBody,
    ShellHeader,
    ShellBody,
    Plan,
    Debug,
    Blank,
}

struct TranscriptLayout {
    lines: Vec<TranscriptLine>,
    first_visible: usize,
}

/// Returns the agent panel under the given screen coordinates, if any.
pub fn panel_at_coords(editor: &Editor, row: u16, column: u16) -> Option<ViewId> {
    editor
        .tree
        .agent_panels()
        .find_map(|(panel, _)| contains_coords(panel.area, row, column).then_some(panel.id))
}

fn contains_coords(area: Rect, row: u16, column: u16) -> bool {
    row >= area.y && row < area.bottom() && column >= area.x && column < area.right()
}

fn panel_layout(area: Rect) -> (Rect, Rect, Rect) {
    let inner = Block::default().borders(Borders::ALL).inner(area);
    if inner.height < INPUT_HEIGHT + HEADER_HEIGHT + 1 {
        return (Rect::default(), Rect::default(), Rect::default());
    }
    let header = Rect {
        height: HEADER_HEIGHT,
        ..inner
    };
    let input = Rect {
        y: inner.bottom().saturating_sub(INPUT_HEIGHT),
        height: INPUT_HEIGHT,
        ..inner
    };
    let transcript = Rect {
        y: inner.y + HEADER_HEIGHT,
        height: inner.height.saturating_sub(INPUT_HEIGHT + HEADER_HEIGHT),
        ..inner
    };
    (header, transcript, input)
}

pub fn handle_mouse(editor: &mut Editor, event: MouseEvent) -> EventResult {
    if matches!(event.kind, MouseEventKind::Moved) {
        return EventResult::Ignored(None);
    }

    let panel_id = match panel_at_coords(editor, event.row, event.column) {
        Some(id) => id,
        None if editor.tree.is_agent_panel(editor.tree.focus) => editor.tree.focus,
        None => return EventResult::Ignored(None),
    };

    editor.tree.focus = panel_id;

    let Some(panel) = editor.tree.agent_panel(panel_id) else {
        return EventResult::Ignored(None);
    };
    let (_header, transcript_area, input) = panel_layout(panel.area);
    let width = transcript_area.width.saturating_sub(2) as usize;
    let layout = build_transcript_layout(editor, width, transcript_area.height as usize);

    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if contains_coords(input, event.row, event.column) {
                editor.agent.focus = AgentFocus::Insert;
                editor.mode = helix_view::document::Mode::Insert;
                editor.agent.clear_transcript_selection();
            } else if contains_coords(transcript_area, event.row, event.column) {
                editor.agent.focus = AgentFocus::Normal;
                editor.mode = helix_view::document::Mode::Normal;
                if let Some(point) =
                    point_from_mouse(transcript_area, event.row, event.column, &layout)
                {
                    editor.agent.transcript_selection = Some(AgentTranscriptSelection::new(point));
                }
            }
            helix_event::request_redraw();
            EventResult::Consumed(None)
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if contains_coords(transcript_area, event.row, event.column) {
                if let Some(point) =
                    point_from_mouse(transcript_area, event.row, event.column, &layout)
                {
                    if let Some(sel) = editor.agent.transcript_selection.as_mut() {
                        sel.head = point;
                        sel.dragging = true;
                    } else {
                        editor.agent.transcript_selection =
                            Some(AgentTranscriptSelection::new(point));
                    }
                }
                helix_event::request_redraw();
            }
            EventResult::Consumed(None)
        }
        MouseEventKind::Up(MouseButton::Left) => {
            let config = editor.config();
            let mut yanked = false;
            let mut toggled_tool = false;
            if let Some(sel) = editor.agent.transcript_selection.as_mut() {
                sel.dragging = false;
                let text = selection_text(&layout.lines, sel);
                if text.is_empty() {
                    let (start, _) = sel.normalized();
                    if let Some(line) = layout.lines.get(start.line) {
                        if matches!(line.kind, TranscriptLineKind::ToolHeader | TranscriptLineKind::ShellHeader) {
                            if let Some(index) = line.entry_index {
                                editor.agent.toggle_collapsible_block(index);
                                editor.agent.block_collapsible_focus = Some(index);
                                toggled_tool = true;
                            }
                        }
                    }
                } else if !text.is_empty() {
                    let mouse_register = config.mouse_yank_register;
                    let clipboard_register = '+';
                    let mouse_result = editor.registers.write(mouse_register, vec![text.clone()]);
                    let clipboard_result = if mouse_register == clipboard_register {
                        Ok(())
                    } else {
                        editor.registers.write(clipboard_register, vec![text])
                    };
                    match (mouse_result, clipboard_result) {
                        (Ok(()), Ok(())) => {
                            let status = if mouse_register == clipboard_register {
                                format!("yanked agent selection to register {mouse_register}")
                            } else {
                                format!(
                                    "yanked agent selection to registers {mouse_register} and {clipboard_register}"
                                )
                            };
                            editor.set_status(status);
                        }
                        (Err(err), _) | (_, Err(err)) => editor.set_error(err.to_string()),
                    }
                    yanked = true;
                }
            }
            if toggled_tool || yanked {
                helix_event::request_redraw();
            }
            EventResult::Consumed(None)
        }
        MouseEventKind::ScrollUp if contains_coords(transcript_area, event.row, event.column) => {
            editor.agent.scroll = editor.agent.scroll.saturating_add(1);
            EventResult::Consumed(None)
        }
        MouseEventKind::ScrollDown if contains_coords(transcript_area, event.row, event.column) => {
            editor.agent.scroll = editor.agent.scroll.saturating_sub(1);
            EventResult::Consumed(None)
        }
        _ => EventResult::Ignored(None),
    }
}

fn build_transcript_layout(
    editor: &Editor,
    width: usize,
    viewport_height: usize,
) -> TranscriptLayout {
    let mut lines = Vec::new();
    if width == 0 {
        return TranscriptLayout {
            lines,
            first_visible: 0,
        };
    }

    let skip = editor.agent.scroll;

    append_debug_lines(width, editor, &mut lines);

    let blocks: Vec<_> = editor.agent.blocks.iter().enumerate().rev().skip(skip).collect();

    for (index, block) in blocks.into_iter().rev() {
        append_block_lines(editor, index, &block.kind, width, &mut lines);
    }

    if editor.agent.blocks.is_empty() && editor.agent.debug_log.is_empty() {
        lines.push(TranscriptLine {
            text: "(empty transcript)".into(),
            spans: Vec::new(),
            kind: TranscriptLineKind::Body {
                thought: true,
                error: false,
            },
            entry_index: None,
        });
    }

    let first_visible = lines.len().saturating_sub(viewport_height);
    TranscriptLayout {
        lines,
        first_visible,
    }
}

fn point_from_mouse(
    area: Rect,
    row: u16,
    column: u16,
    layout: &TranscriptLayout,
) -> Option<AgentTranscriptPoint> {
    let rel_row = row.saturating_sub(area.y) as usize;
    let visible_start = layout.first_visible;
    let visible_end = layout.first_visible + area.height as usize;
    if rel_row >= area.height as usize {
        return None;
    }
    let line = visible_start + rel_row;
    if line >= layout.lines.len() || line >= visible_end {
        return None;
    }
    let rel_col = column.saturating_sub(area.x) as usize;
    let text = line_display_text(&layout.lines[line]);
    let col = col_from_display_x(&text, rel_col);
    Some(AgentTranscriptPoint { line, col })
}

fn line_display_text(line: &TranscriptLine) -> String {
    if !line.text.is_empty() {
        return line.text.clone();
    }
    line.spans
        .iter()
        .map(|span| span.text.as_str())
        .collect()
}

fn col_from_display_x(text: &str, target_x: usize) -> usize {
    let mut x = 0;
    let mut col = 0;
    for g in text.graphemes(true) {
        let w = g.width();
        if x >= target_x {
            break;
        }
        x += w;
        col += 1;
    }
    col.min(text.chars().count())
}

fn selection_text(lines: &[TranscriptLine], sel: &AgentTranscriptSelection) -> String {
    let (start, end) = sel.normalized();
    if start.line >= lines.len() {
        return String::new();
    }
    if start.line == end.line && start.col == end.col {
        return String::new();
    }

    let mut out = String::new();
    for (idx, line) in lines
        .iter()
        .enumerate()
        .skip(start.line)
        .take(end.line.saturating_sub(start.line) + 1)
    {
        if idx > start.line {
            out.push('\n');
        }
        let end_col = end.col.saturating_add(1);
        let line_text = line_display_text(line);
        let piece: String = if start.line == end.line {
            chars_range(&line_text, start.col, end_col)
        } else if idx == start.line {
            chars_from(&line_text, start.col)
        } else if idx == end.line {
            chars_until(&line_text, end_col)
        } else {
            line_text
        };
        out.push_str(&piece);
    }
    out
}

fn chars_range(text: &str, start: usize, end: usize) -> String {
    if start >= end {
        return String::new();
    }
    text.chars()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect()
}

fn chars_from(text: &str, start: usize) -> String {
    text.chars().skip(start).collect()
}

fn chars_until(text: &str, end: usize) -> String {
    text.chars().take(end).collect()
}

pub fn render(editor: &Editor, area: Rect, surface: &mut Surface, focused: bool) {
    let theme = &editor.theme;
    let border_style = if focused {
        theme.get("ui.selection.active")
    } else {
        theme.get("ui.selection")
    };
    let label_style = theme.get("ui.text");
    let role_style = theme.get("ui.help");
    let text_style = theme.get("ui.text");
    let error_style = theme.get("error");
    let thought_style = theme.get("ui.text.inactive");
    let input_style = theme.get("ui.text");
    let prompt_style = theme.get("ui.text.inactive");
    let selection_style = theme.get("ui.selection");

    let title = match active_session_title(editor) {
        Some(title) => format!(" Agent: {title} "),
        None => " Agent ".to_string(),
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Span::styled(
            title,
            label_style.add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    block.render(area, surface);

    if inner.height < INPUT_HEIGHT + HEADER_HEIGHT + 1 {
        return;
    }

    let header_area = Rect {
        height: HEADER_HEIGHT,
        ..inner
    };
    let input_area = Rect {
        y: inner.bottom().saturating_sub(INPUT_HEIGHT),
        height: INPUT_HEIGHT,
        ..inner
    };
    let transcript_area = Rect {
        y: inner.y + HEADER_HEIGHT,
        height: inner.height.saturating_sub(INPUT_HEIGHT + HEADER_HEIGHT),
        ..inner
    };

    let status = editor
        .agent
        .status
        .as_deref()
        .unwrap_or(if editor.agent.pending {
            "thinking..."
        } else {
            "ready"
        });
    let mode = editor
        .agent
        .mode
        .as_deref()
        .map(|mode| format!("mode:{mode}"))
        .unwrap_or_default();
    let session = active_session_title(editor)
        .or(editor.agent.active_session.as_deref().map(short_session_id))
        .unwrap_or("no session");
    let header = if mode.is_empty() {
        format!(
            "{} — {} — {}",
            session,
            status,
            match editor.agent.focus {
                AgentFocus::Insert => "INSERT",
                AgentFocus::Normal => "NORMAL",
            }
        )
    } else {
        format!(
            "{} — {} — {} — {}",
            session,
            mode,
            status,
            match editor.agent.focus {
                AgentFocus::Insert => "INSERT",
                AgentFocus::Normal => "NORMAL",
            }
        )
    };
    surface.set_stringn(
        header_area.x,
        header_area.y,
        &header,
        header_area.width as usize,
        label_style,
    );

    render_transcript(
        editor,
        transcript_area,
        surface,
        role_style,
        text_style,
        error_style,
        thought_style,
        selection_style,
    );
    render_input(
        editor,
        input_area,
        surface,
        prompt_style,
        input_style,
        focused && editor.agent.focus == AgentFocus::Insert,
    );
}

fn short_session_id(session_id: &str) -> &str {
    session_id
        .char_indices()
        .nth(8)
        .map(|(idx, _)| &session_id[..idx])
        .unwrap_or(session_id)
}

fn active_session_title(editor: &Editor) -> Option<&str> {
    let active_session = editor.agent.active_session.as_deref()?;
    editor
        .agent
        .sessions
        .iter()
        .find(|session| session.id == active_session)
        .and_then(|session| session.title.as_deref())
        .filter(|title| !title.trim().is_empty())
}

#[allow(clippy::too_many_arguments)]
fn render_transcript(
    editor: &Editor,
    area: Rect,
    surface: &mut Surface,
    role_style: Style,
    text_style: Style,
    error_style: Style,
    thought_style: Style,
    selection_style: Style,
) {
    let width = area.width.saturating_sub(2) as usize;
    if width == 0 || area.height == 0 {
        return;
    }

    let layout = build_transcript_layout(editor, width, area.height as usize);
    let selection = editor.agent.transcript_selection;

    for row in 0..area.height as usize {
        let line_idx = layout.first_visible + row;
        let Some(line) = layout.lines.get(line_idx) else {
            break;
        };
        let y = area.y + row as u16;
        let base_style = line_style(
            line.kind,
            role_style,
            text_style,
            error_style,
            thought_style,
        );
        render_line_with_selection(
            surface,
            area.x,
            y,
            area.width,
            line,
            base_style,
            selection_style,
            selection.as_ref(),
            line_idx,
        );
    }
}

fn append_debug_lines(width: usize, editor: &Editor, lines: &mut Vec<TranscriptLine>) {
    if editor.agent.debug_log.is_empty() {
        return;
    }

    lines.push(TranscriptLine {
        text: "— agent debug —".into(),
        spans: Vec::new(),
        kind: TranscriptLineKind::Debug,
        entry_index: None,
    });
    for entry in &editor.agent.debug_log {
        for wrapped in wrap_text(entry, width) {
            lines.push(TranscriptLine {
                text: wrapped,
                spans: Vec::new(),
                kind: TranscriptLineKind::Debug,
                entry_index: None,
            });
        }
    }
    lines.push(blank_line());
}

fn line_style(
    kind: TranscriptLineKind,
    role_style: Style,
    text_style: Style,
    error_style: Style,
    thought_style: Style,
) -> Style {
    match kind {
        TranscriptLineKind::Role
        | TranscriptLineKind::ToolHeader
        | TranscriptLineKind::ToolBody
        | TranscriptLineKind::ShellHeader
        | TranscriptLineKind::ShellBody
        | TranscriptLineKind::Plan => role_style,
        TranscriptLineKind::Debug => thought_style.add_modifier(Modifier::ITALIC),
        TranscriptLineKind::Body { thought: true, .. } => thought_style,
        TranscriptLineKind::Body { error: true, .. } => error_style,
        TranscriptLineKind::Body { .. } | TranscriptLineKind::Blank => text_style,
    }
}

#[allow(clippy::too_many_arguments)]
fn render_line_with_selection(
    surface: &mut Surface,
    x: u16,
    y: u16,
    width: u16,
    line: &TranscriptLine,
    base_style: Style,
    selection_style: Style,
    selection: Option<&AgentTranscriptSelection>,
    line_idx: usize,
) {
    let (sel_start, sel_end) = selection_chars_for_line(selection, line_idx);
    let mut char_idx = 0usize;
    let mut display_x = 0u16;
    if line.spans.is_empty() {
        render_text_segment(
            surface,
            x,
            y,
            width,
            &line.text,
            base_style,
            selection_style,
            sel_start,
            sel_end,
            &mut char_idx,
            &mut display_x,
        );
    } else {
        for span in &line.spans {
            render_text_segment(
                surface,
                x,
                y,
                width,
                &span.text,
                span.style,
                selection_style,
                sel_start,
                sel_end,
                &mut char_idx,
                &mut display_x,
            );
            if display_x >= width {
                break;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_text_segment(
    surface: &mut Surface,
    x: u16,
    y: u16,
    width: u16,
    text: &str,
    base_style: Style,
    selection_style: Style,
    sel_start: Option<usize>,
    sel_end: usize,
    char_idx: &mut usize,
    display_x: &mut u16,
) {
    for g in text.graphemes(true) {
        if *display_x >= width {
            break;
        }
        let g_width = g.width() as u16;
        let selected = sel_start
            .map(|start| *char_idx >= start && *char_idx < sel_end)
            .unwrap_or(false);
        let style = if selected {
            selection_style
        } else {
            base_style
        };
        surface.set_stringn(x + *display_x, y, g, g_width as usize, style);
        *display_x += g_width;
        *char_idx += 1;
    }
}

fn selection_chars_for_line(
    selection: Option<&AgentTranscriptSelection>,
    line: usize,
) -> (Option<usize>, usize) {
    let Some(sel) = selection else {
        return (None, 0);
    };
    let (start, end) = sel.normalized();
    if line < start.line || line > end.line {
        return (None, 0);
    }
    let start_col = if line == start.line { start.col } else { 0 };
    let end_col = if line == end.line {
        end.col.saturating_add(1)
    } else {
        usize::MAX
    };
    (Some(start_col), end_col)
}

fn append_block_lines(
    editor: &Editor,
    block_index: usize,
    kind: &AgentBlockKind,
    width: usize,
    lines: &mut Vec<TranscriptLine>,
) {
    match kind {
        AgentBlockKind::User { text }
        | AgentBlockKind::Assistant { text }
        | AgentBlockKind::Thought { text } => {
            let thought = matches!(kind, AgentBlockKind::Thought { .. });
            lines.push(TranscriptLine {
                text: format!("{}: ", kind.role_label()),
                spans: Vec::new(),
                kind: TranscriptLineKind::Role,
                entry_index: None,
            });
            append_markdown_lines(
                editor,
                text,
                width,
                TranscriptLineKind::Body {
                    thought,
                    error: false,
                },
                lines,
            );
            lines.push(blank_line());
        }
        AgentBlockKind::Tool {
            title,
            status,
            detail,
            shell_output,
            expanded,
            ..
        } => {
            let marker = if *expanded { "▾" } else { "▸" };
            lines.push(TranscriptLine {
                text: format!("{marker} tool: {title} [{status}]"),
                spans: Vec::new(),
                kind: TranscriptLineKind::ToolHeader,
                entry_index: Some(block_index),
            });
            if *expanded {
                if !shell_output.is_empty() {
                    for wrapped in wrap_text(shell_output, width.saturating_sub(2)) {
                        lines.push(TranscriptLine {
                            text: format!("  {wrapped}"),
                            spans: Vec::new(),
                            kind: TranscriptLineKind::ShellBody,
                            entry_index: Some(block_index),
                        });
                    }
                } else if let Some(detail) = detail.as_deref().filter(|text| !text.is_empty()) {
                    for wrapped in wrap_text(detail, width.saturating_sub(2)) {
                        lines.push(TranscriptLine {
                            text: format!("  {wrapped}"),
                            spans: Vec::new(),
                            kind: TranscriptLineKind::ToolBody,
                            entry_index: Some(block_index),
                        });
                    }
                }
            }
            lines.push(blank_line());
        }
        AgentBlockKind::Shell {
            command,
            args,
            output,
            status,
            exit_code,
            expanded,
            ..
        } => {
            let marker = if *expanded { "▾" } else { "▸" };
            let status_label = match status {
                ShellBlockStatus::Running => "running".to_string(),
                ShellBlockStatus::Completed => exit_code
                    .map(|code| format!("exit {code}"))
                    .unwrap_or_else(|| "completed".into()),
                ShellBlockStatus::Failed => exit_code
                    .map(|code| format!("failed ({code})"))
                    .unwrap_or_else(|| "failed".into()),
            };
            let cmd_line = if args.is_empty() {
                command.clone()
            } else {
                format!("{command} {}", args.join(" "))
            };
            lines.push(TranscriptLine {
                text: format!("{marker} $ {cmd_line} [{status_label}]"),
                spans: Vec::new(),
                kind: TranscriptLineKind::ShellHeader,
                entry_index: Some(block_index),
            });
            if *expanded && !output.is_empty() {
                for wrapped in wrap_text(output, width.saturating_sub(2)) {
                    lines.push(TranscriptLine {
                        text: format!("  {wrapped}"),
                        spans: Vec::new(),
                        kind: TranscriptLineKind::ShellBody,
                        entry_index: Some(block_index),
                    });
                }
            }
            lines.push(blank_line());
        }
        AgentBlockKind::Plan { entries } => {
            lines.push(TranscriptLine {
                text: "plan".into(),
                spans: Vec::new(),
                kind: TranscriptLineKind::Plan,
                entry_index: None,
            });
            for item in entries {
                for wrapped in wrap_text(item, width.saturating_sub(2)) {
                    lines.push(TranscriptLine {
                        text: format!("  {wrapped}"),
                        spans: Vec::new(),
                        kind: TranscriptLineKind::Plan,
                        entry_index: None,
                    });
                }
            }
            lines.push(blank_line());
        }
        AgentBlockKind::System { text } => {
            lines.push(TranscriptLine {
                text: "system: ".into(),
                spans: Vec::new(),
                kind: TranscriptLineKind::Role,
                entry_index: None,
            });
            append_markdown_lines(
                editor,
                text,
                width,
                TranscriptLineKind::Body {
                    thought: false,
                    error: false,
                },
                lines,
            );
            lines.push(blank_line());
        }
        AgentBlockKind::Error { text } => {
            lines.push(TranscriptLine {
                text: "error: ".into(),
                spans: Vec::new(),
                kind: TranscriptLineKind::Role,
                entry_index: None,
            });
            for wrapped in wrap_text(text, width) {
                lines.push(TranscriptLine {
                    text: wrapped,
                    spans: Vec::new(),
                    kind: TranscriptLineKind::Body {
                        thought: false,
                        error: true,
                    },
                    entry_index: None,
                });
            }
            lines.push(blank_line());
        }
    }
}

fn blank_line() -> TranscriptLine {
    TranscriptLine {
        text: String::new(),
        spans: Vec::new(),
        kind: TranscriptLineKind::Blank,
        entry_index: None,
    }
}

fn append_markdown_lines(
    editor: &Editor,
    text: &str,
    width: usize,
    kind: TranscriptLineKind,
    lines: &mut Vec<TranscriptLine>,
) {
    let markdown = Markdown::new(text.to_string(), editor.syn_loader.clone());
    let rendered = markdown.parse(Some(&editor.theme));
    if rendered.lines.is_empty() {
        lines.push(TranscriptLine {
            text: String::new(),
            spans: Vec::new(),
            kind,
            entry_index: None,
        });
        return;
    }

    for line in rendered.lines {
        if line.0.is_empty() {
            lines.push(blank_line());
            continue;
        }
        append_wrapped_spans(line, width, kind, lines);
    }
}

fn append_wrapped_spans(
    spans: Spans<'_>,
    width: usize,
    kind: TranscriptLineKind,
    lines: &mut Vec<TranscriptLine>,
) {
    if width == 0 {
        lines.push(TranscriptLine {
            text: spans.0.iter().map(|span| span.content.as_ref()).collect(),
            spans: spans
                .0
                .into_iter()
                .map(|span| TranscriptSpan {
                    text: span.content.into_owned(),
                    style: span.style,
                })
                .collect(),
            kind,
            entry_index: None,
        });
        return;
    }

    let mut current = Vec::new();
    let mut current_width = 0usize;
    for span in spans.0 {
        let style = span.style;
        for grapheme in span.content.graphemes(true) {
            let grapheme_width = grapheme.width();
            if current_width > 0 && current_width + grapheme_width > width {
                push_styled_line(&mut current, kind, lines);
                current_width = 0;
            }
            push_styled_segment(&mut current, grapheme, style);
            current_width += grapheme_width;
        }
    }

    push_styled_line(&mut current, kind, lines);
}

fn push_styled_segment(spans: &mut Vec<TranscriptSpan>, text: &str, style: Style) {
    if let Some(last) = spans.last_mut() {
        if last.style == style {
            last.text.push_str(text);
            return;
        }
    }
    spans.push(TranscriptSpan {
        text: text.to_string(),
        style,
    });
}

fn push_styled_line(
    spans: &mut Vec<TranscriptSpan>,
    kind: TranscriptLineKind,
    lines: &mut Vec<TranscriptLine>,
) {
    let spans = std::mem::take(spans);
    let text = spans.iter().map(|span| span.text.as_str()).collect();
    lines.push(TranscriptLine {
        text,
        spans,
        kind,
        entry_index: None,
    });
}

fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let extra = if current.is_empty() {
            word.width()
        } else {
            1 + word.width()
        };
        if !current.is_empty() && current.width() + extra > width {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn render_input(
    editor: &Editor,
    area: Rect,
    surface: &mut Surface,
    prompt_style: Style,
    input_style: Style,
    focused: bool,
) {
    surface.set_string(area.x, area.y, "› ", prompt_style);
    let input_x = area.x + 2;
    let input_width = area.width.saturating_sub(2) as usize;
    let display = truncate_start(&editor.agent.input, input_width);
    let input_style = if focused {
        input_style
    } else {
        input_style.add_modifier(Modifier::DIM)
    };
    surface.set_stringn(input_x, area.y, &display, input_width, input_style);

    if focused {
        let cursor_col = grapheme_width_before(&editor.agent.input, editor.agent.input_cursor);
        let offset = editor.agent.input.width().saturating_sub(display.width()) as u16;
        let cursor_x = input_x + cursor_col.saturating_sub(offset);
        if let Some(cell) = surface.get_mut(cursor_x.min(area.right().saturating_sub(1)), area.y) {
            cell.set_style(input_style.add_modifier(Modifier::REVERSED));
        }
    }

    if area.height > 1 {
        let hint = if focused {
            if editor.agent.pending {
                "INSERT · Enter send · Esc normal · drag select"
            } else {
                "INSERT · Enter send · Esc normal · ↑↓ history"
            }
        } else {
            "NORMAL · i edit · C-w/Space-w switch panes · : commands"
        };
        surface.set_stringn(area.x, area.y + 1, hint, area.width as usize, prompt_style);
    }
}

fn truncate_start(text: &str, max_width: usize) -> String {
    if text.width() <= max_width {
        return text.to_string();
    }
    let mut width = 0;
    let mut start = text.len();
    for g in text.graphemes(true).rev() {
        width += g.width();
        if width > max_width {
            break;
        }
        start -= g.len();
    }
    text[start..].to_string()
}

fn grapheme_width_before(text: &str, byte_index: usize) -> u16 {
    text[..byte_index.min(text.len())].width() as u16
}

pub fn handle_key(editor: &mut Editor, key: KeyEvent) -> bool {
    if !editor.agent.is_open() {
        return false;
    }

    match key.code {
        KeyCode::Esc => {
            editor.agent.focus = AgentFocus::Normal;
            editor.mode = helix_view::document::Mode::Normal;
            helix_event::request_redraw();
            true
        }
        KeyCode::Enter => {
            let text = editor.agent.input.trim().to_string();
            if !text.is_empty() {
                crate::agent::with_controller(|controller| {
                    crate::handlers::agent::send_prompt(controller, editor, text);
                });
            }
            true
        }
        KeyCode::Backspace => {
            if editor.agent.input_cursor > 0 {
                let prev = prev_grapheme_boundary(&editor.agent.input, editor.agent.input_cursor);
                editor
                    .agent
                    .input
                    .replace_range(prev..editor.agent.input_cursor, "");
                editor.agent.input_cursor = prev;
            }
            true
        }
        KeyCode::Delete => {
            if editor.agent.input_cursor < editor.agent.input.len() {
                let next = next_grapheme_boundary(&editor.agent.input, editor.agent.input_cursor);
                editor
                    .agent
                    .input
                    .replace_range(editor.agent.input_cursor..next, "");
            }
            true
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            editor.agent.input.clear();
            editor.agent.input_cursor = 0;
            true
        }
        KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            delete_word_backward(editor);
            true
        }
        KeyCode::Char(c) if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT => {
            editor.agent.clear_transcript_selection();
            editor.agent.input.insert(editor.agent.input_cursor, c);
            editor.agent.input_cursor += c.len_utf8();
            true
        }
        KeyCode::Left => {
            editor.agent.input_cursor =
                prev_grapheme_boundary(&editor.agent.input, editor.agent.input_cursor);
            true
        }
        KeyCode::Right => {
            editor.agent.input_cursor =
                next_grapheme_boundary(&editor.agent.input, editor.agent.input_cursor);
            true
        }
        KeyCode::Home => {
            editor.agent.input_cursor = 0;
            true
        }
        KeyCode::End => {
            editor.agent.input_cursor = editor.agent.input.len();
            true
        }
        KeyCode::Up => {
            history_prev(editor);
            true
        }
        KeyCode::Down => {
            history_next(editor);
            true
        }
        KeyCode::PageUp => {
            editor.agent.scroll = editor.agent.scroll.saturating_add(1);
            true
        }
        KeyCode::PageDown => {
            editor.agent.scroll = editor.agent.scroll.saturating_sub(1);
            true
        }
        _ => false,
    }
}

pub(crate) fn enter_insert_mode(editor: &mut Editor) {
    editor.agent.focus = AgentFocus::Insert;
    editor.mode = helix_view::document::Mode::Insert;
    helix_event::request_redraw();
}

pub(crate) fn toggle_focused_collapsible_block(editor: &mut Editor) {
    toggle_focused_collapsible(editor);
}

fn toggle_focused_collapsible(editor: &mut Editor) -> bool {
    let index = editor
        .agent
        .block_collapsible_focus
        .or_else(|| editor.agent.collapsible_block_indices().last());
    let Some(index) = index else {
        return false;
    };
    editor.agent.toggle_collapsible_block(index);
    editor.agent.block_collapsible_focus = Some(index);
    helix_event::request_redraw();
    true
}

fn prev_grapheme_boundary(text: &str, index: usize) -> usize {
    text.grapheme_indices(true)
        .rev()
        .find_map(|(i, _)| (i < index).then_some(i))
        .unwrap_or(0)
}

fn next_grapheme_boundary(text: &str, index: usize) -> usize {
    text.grapheme_indices(true)
        .find_map(|(i, _)| (i > index).then_some(i))
        .unwrap_or(text.len())
}

fn delete_word_backward(editor: &mut Editor) {
    if editor.agent.input_cursor == 0 {
        return;
    }
    let before = &editor.agent.input[..editor.agent.input_cursor];
    let mut boundary = before.len();
    let mut chars = before.chars().rev().peekable();
    for c in chars.by_ref() {
        if !c.is_whitespace() {
            break;
        }
        boundary -= c.len_utf8();
    }
    for c in chars {
        if c.is_whitespace() {
            break;
        }
        boundary -= c.len_utf8();
    }
    editor
        .agent
        .input
        .replace_range(boundary..editor.agent.input_cursor, "");
    editor.agent.input_cursor = boundary;
}

fn history_prev(editor: &mut Editor) {
    let len = editor.agent.prompt_history.len();
    if len == 0 {
        return;
    }
    let index = editor.agent.history_pos.map_or(0, |i| (i + 1).min(len - 1));
    editor.agent.history_pos = Some(index);
    if let Some(text) = editor.agent.prompt_history.get(index) {
        editor.agent.input.clone_from(text);
        editor.agent.input_cursor = editor.agent.input.len();
    }
}

fn history_next(editor: &mut Editor) {
    let Some(index) = editor.agent.history_pos else {
        return;
    };
    if index == 0 {
        editor.agent.history_pos = None;
        editor.agent.input.clear();
        editor.agent.input_cursor = 0;
    } else {
        let index = index - 1;
        editor.agent.history_pos = Some(index);
        if let Some(text) = editor.agent.prompt_history.get(index) {
            editor.agent.input.clone_from(text);
            editor.agent.input_cursor = editor.agent.input.len();
        }
    }
}

pub fn cursor(editor: &Editor, _area: Rect) -> (Option<Position>, CursorKind) {
    if !editor.agent_panel_focused() || !editor.agent.is_open() {
        return (None, CursorKind::Hidden);
    }

    if let Some((panel, _)) = editor.tree.agent_panels().find(|(_, focused)| *focused) {
        let inner = Block::default().borders(Borders::ALL).inner(panel.area);
        let input_y = inner.bottom().saturating_sub(INPUT_HEIGHT);
        let input_x = inner.x + 2;
        let input_width = inner.width.saturating_sub(2) as usize;
        let display = truncate_start(&editor.agent.input, input_width);
        let offset = editor.agent.input.width().saturating_sub(display.width()) as u16;
        let cursor_col = grapheme_width_before(&editor.agent.input, editor.agent.input_cursor)
            .saturating_sub(offset);
        return (
            Some(Position::new(
                input_y as usize,
                (input_x + cursor_col.min(inner.width.saturating_sub(3))) as usize,
            )),
            CursorKind::Block,
        );
    }

    (None, CursorKind::Hidden)
}
