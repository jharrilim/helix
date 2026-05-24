use crate::compositor::EventResult;
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::color::Colors;
use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};
use helix_core::unicode::width::UnicodeWidthChar;
use helix_core::Position;
use helix_pty::TerminalScroll;
use helix_view::{
    graphics::{Color as HelixColor, CursorKind, Modifier, Rect, Style},
    input::{KeyEvent, MouseButton, MouseEvent, MouseEventKind},
    keyboard::{KeyCode, KeyModifiers},
    terminal::TerminalFocus,
    Editor, ViewId,
};
use tui::buffer::Buffer as Surface;
use tui::text::Span;
use tui::widgets::{Block, Borders, Widget};

const HEADER_HEIGHT: u16 = 1;

/// Returns the terminal panel under the given screen coordinates, if any.
pub fn panel_at_coords(editor: &Editor, row: u16, column: u16) -> Option<ViewId> {
    editor.tree.terminal_panels().find_map(|(panel, _)| {
        contains_coords(panel.area, row, column).then_some(panel.id)
    })
}

fn contains_coords(area: Rect, row: u16, column: u16) -> bool {
    row >= area.y && row < area.bottom() && column >= area.x && column < area.right()
}

pub fn grid_area(area: Rect) -> Rect {
    let inner = Block::default().borders(Borders::ALL).inner(area);
    if inner.height <= HEADER_HEIGHT {
        return Rect::default();
    }
    Rect {
        y: inner.y + HEADER_HEIGHT,
        height: inner.height.saturating_sub(HEADER_HEIGHT),
        ..inner
    }
}

pub fn grid_size(area: Rect) -> (u16, u16) {
    let grid = grid_area(area);
    (grid.height.max(1), grid.width.max(2))
}

pub fn render(editor: &Editor, area: Rect, surface: &mut Surface, focused: bool) {
    let session_id = editor
        .tree
        .terminal_panels()
        .find(|(panel, is_focused)| panel.area == area && *is_focused == focused)
        .map(|(panel, _)| panel.session_id.as_str())
        .or_else(|| {
            editor
                .tree
                .terminal_panels()
                .find(|(panel, _)| panel.area == area)
                .map(|(panel, _)| panel.session_id.as_str())
        })
        .or(editor.terminal.active_session.as_deref());

    render_panel(editor, area, surface, focused, session_id);
}

fn render_panel(
    editor: &Editor,
    area: Rect,
    surface: &mut Surface,
    focused: bool,
    session_id: Option<&str>,
) {
    let theme = &editor.theme;
    let border_style = if focused {
        theme.get("ui.selection.active")
    } else {
        theme.get("ui.selection")
    };
    let label_style = theme.get("ui.text");
    let header_style = theme.get("ui.text.inactive");

    let Some(session_id) = session_id else {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(Span::styled(" Terminal ", label_style.add_modifier(Modifier::BOLD)));
        block.render(area, surface);
        return;
    };

    let session = editor.terminal.sessions.get(session_id);
    let title = session
        .and_then(|session| session.title.as_deref())
        .unwrap_or(session_id);
    let cwd = session
        .map(|session| session.cwd.display().to_string())
        .unwrap_or_default();
    let exit = session.and_then(|session| session.exit_status);

    let mode = if focused && editor.terminal.focus == TerminalFocus::Insert {
        "I"
    } else {
        "N"
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Span::styled(
            format!(" Terminal: {title} "),
            label_style.add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    block.render(area, surface);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let header_area = Rect {
        height: HEADER_HEIGHT.min(inner.height),
        ..inner
    };
    let body = Rect {
        y: inner.y + header_area.height,
        height: inner.height.saturating_sub(header_area.height),
        ..inner
    };

    let header = if let Some(code) = exit {
        format!("[{mode}] {session_id} — {cwd} — exited {code}")
    } else {
        format!("[{mode}] {session_id} — {cwd}")
    };
    surface.set_string(header_area.x, header_area.y, &header, header_style);

    let Some(handle) = crate::terminal::session_handle(session_id) else {
        surface.set_string(body.x, body.y, "starting...", label_style);
        return;
    };

    let default_fg = theme.get("ui.text");
    let default_bg = theme.get("ui.background");

    handle.with_term(|term| {
        let content = term.renderable_content();
        let cols = body.width as usize;
        let rows = body.height as usize;
        if cols == 0 || rows == 0 {
            return;
        }

        let mut row = 0usize;
        let mut col = 0usize;
        for indexed in content.display_iter {
            if row >= rows {
                break;
            }

            let cell: &Cell = indexed.cell;
            if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                continue;
            }

            let c = cell.c;
            let style = cell_to_style(cell, &content.colors, default_fg, default_bg);
            let width = c.width().unwrap_or(1);
            for offset in 0..width {
                if col >= cols {
                    col = 0;
                    row += 1;
                    if row >= rows {
                        return;
                    }
                }
                let x = body.x + col as u16;
                let y = body.y + row as u16;
                if offset == 0 {
                    surface[(x, y)].set_symbol(&c.to_string()).set_style(style);
                } else {
                    surface[(x, y)].set_symbol(" ".into()).set_style(style);
                }
                col += 1;
            }
        }
    });
}

fn cell_to_style(cell: &Cell, colors: &Colors, default_fg: Style, default_bg: Style) -> Style {
    let mut fg = ansi_color_to_style(cell.fg, colors, default_fg, true);
    let mut bg = ansi_color_to_style(cell.bg, colors, default_bg, false);

    if cell.flags.contains(Flags::INVERSE) {
        std::mem::swap(&mut fg, &mut bg);
    }

    let mut style = fg;
    if let Some(color) = bg.bg {
        style = style.bg(color);
    }
    if cell.flags.contains(Flags::BOLD) || cell.flags.contains(Flags::DIM_BOLD) {
        style.add_modifier |= Modifier::BOLD;
    }
    if cell.flags.contains(Flags::ITALIC) {
        style.add_modifier |= Modifier::ITALIC;
    }
    if cell.flags.intersects(Flags::ALL_UNDERLINES) {
        style = style.underline_color(fg.fg.unwrap_or(HelixColor::Reset));
    }
    if cell.flags.contains(Flags::HIDDEN) {
        style.add_modifier |= Modifier::HIDDEN;
    }
    if cell.flags.contains(Flags::STRIKEOUT) {
        style.add_modifier |= Modifier::CROSSED_OUT;
    }

    style
}

fn ansi_color_to_style(
    color: Color,
    colors: &Colors,
    default: Style,
    foreground: bool,
) -> Style {
    let helix_color = match color {
        Color::Named(named) => match named {
            NamedColor::Foreground => default.fg.unwrap_or(HelixColor::Reset),
            NamedColor::Background => default.bg.unwrap_or(HelixColor::Reset),
            other => named_color(other),
        },
        Color::Spec(Rgb { r, g, b }) => HelixColor::Rgb(r, g, b),
        Color::Indexed(index) => indexed_color(index, colors),
    };

    if foreground {
        Style::default().fg(helix_color)
    } else {
        Style::default().bg(helix_color)
    }
}

fn named_color(color: NamedColor) -> HelixColor {
    match color {
        NamedColor::Black => HelixColor::Rgb(0, 0, 0),
        NamedColor::Red => HelixColor::Rgb(205, 0, 0),
        NamedColor::Green => HelixColor::Rgb(0, 205, 0),
        NamedColor::Yellow => HelixColor::Rgb(205, 205, 0),
        NamedColor::Blue => HelixColor::Rgb(0, 0, 238),
        NamedColor::Magenta => HelixColor::Rgb(205, 0, 205),
        NamedColor::Cyan => HelixColor::Rgb(0, 205, 205),
        NamedColor::White => HelixColor::Rgb(229, 229, 229),
        NamedColor::BrightBlack => HelixColor::Rgb(127, 127, 127),
        NamedColor::BrightRed => HelixColor::Rgb(255, 0, 0),
        NamedColor::BrightGreen => HelixColor::Rgb(0, 255, 0),
        NamedColor::BrightYellow => HelixColor::Rgb(255, 255, 0),
        NamedColor::BrightBlue => HelixColor::Rgb(92, 92, 255),
        NamedColor::BrightMagenta => HelixColor::Rgb(255, 0, 255),
        NamedColor::BrightCyan => HelixColor::Rgb(0, 255, 255),
        NamedColor::BrightWhite => HelixColor::Rgb(255, 255, 255),
        NamedColor::DimBlack => HelixColor::Rgb(0, 0, 0),
        NamedColor::DimRed => HelixColor::Rgb(205, 0, 0),
        NamedColor::DimGreen => HelixColor::Rgb(0, 205, 0),
        NamedColor::DimYellow => HelixColor::Rgb(205, 205, 0),
        NamedColor::DimBlue => HelixColor::Rgb(0, 0, 238),
        NamedColor::DimMagenta => HelixColor::Rgb(205, 0, 205),
        NamedColor::DimCyan => HelixColor::Rgb(0, 205, 205),
        NamedColor::DimWhite => HelixColor::Rgb(229, 229, 229),
        NamedColor::DimForeground | NamedColor::BrightForeground => HelixColor::Reset,
        NamedColor::Foreground | NamedColor::Background | NamedColor::Cursor => HelixColor::Reset,
    }
}

fn indexed_color(index: u8, colors: &Colors) -> HelixColor {
    if let Some(rgb) = colors[index as usize] {
        return HelixColor::Rgb(rgb.r, rgb.g, rgb.b);
    }

    if index < 16 {
        return named_color(match index {
            0 => NamedColor::Black,
            1 => NamedColor::Red,
            2 => NamedColor::Green,
            3 => NamedColor::Yellow,
            4 => NamedColor::Blue,
            5 => NamedColor::Magenta,
            6 => NamedColor::Cyan,
            7 => NamedColor::White,
            8 => NamedColor::BrightBlack,
            9 => NamedColor::BrightRed,
            10 => NamedColor::BrightGreen,
            11 => NamedColor::BrightYellow,
            12 => NamedColor::BrightBlue,
            13 => NamedColor::BrightMagenta,
            14 => NamedColor::BrightCyan,
            _ => NamedColor::BrightWhite,
        });
    }

    if index < 232 {
        let index = index - 16;
        let r = index / 36;
        let g = (index % 36) / 6;
        let b = index % 6;
        let ramp = |step: u8| if step == 0 { 0 } else { 55 + step * 40 };
        return HelixColor::Rgb(ramp(r), ramp(g), ramp(b));
    }

    let gray = 8 + (index - 232) * 10;
    HelixColor::Rgb(gray, gray, gray)
}

pub fn handle_mouse(editor: &mut Editor, event: MouseEvent) -> EventResult {
    if matches!(event.kind, MouseEventKind::Moved) {
        return EventResult::Ignored(None);
    }

    let panel_id = match panel_at_coords(editor, event.row, event.column) {
        Some(id) => id,
        None if editor.tree.is_terminal_panel(editor.tree.focus) => editor.tree.focus,
        None => return EventResult::Ignored(None),
    };

    editor.tree.focus = panel_id;
    if let Some(panel) = editor.tree.terminal_panel(panel_id) {
        editor.terminal.active_session = Some(panel.session_id.clone());
    }

    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            editor.terminal.focus = TerminalFocus::Insert;
            editor.mode = helix_view::document::Mode::Insert;
            helix_event::request_redraw();
            EventResult::Consumed(None)
        }
        MouseEventKind::ScrollUp => {
            scroll_lines(editor, 1);
            EventResult::Consumed(None)
        }
        MouseEventKind::ScrollDown => {
            scroll_lines(editor, -1);
            EventResult::Consumed(None)
        }
        _ => EventResult::Ignored(None),
    }
}

pub fn handle_key(editor: &mut Editor, key: KeyEvent) -> bool {
    if !editor.terminal.is_open() {
        return false;
    }

    if key.code == KeyCode::Esc {
        editor.terminal.focus = TerminalFocus::Normal;
        editor.mode = helix_view::document::Mode::Normal;
        helix_event::request_redraw();
        return true;
    }

    let Some(session_id) = active_session_id(editor) else {
        return false;
    };

    let bytes = encode_key(key);
    if bytes.is_empty() {
        return false;
    }

    crate::terminal::send(helix_pty::TerminalCommand::Write {
        id: session_id.into(),
        data: bytes,
    });
    true
}

pub fn handle_normal_key(editor: &mut Editor, key: KeyEvent) -> bool {
    if !editor.terminal.is_open() {
        return false;
    }

    match key.code {
        KeyCode::Char('i') | KeyCode::Char('a') => {
            editor.terminal.pending_scroll_top = false;
            editor.terminal.focus = TerminalFocus::Insert;
            editor.mode = helix_view::document::Mode::Insert;
            helix_event::request_redraw();
            true
        }
        KeyCode::Char('j') => {
            editor.terminal.pending_scroll_top = false;
            scroll_lines(editor, -1);
            true
        }
        KeyCode::Char('k') => {
            editor.terminal.pending_scroll_top = false;
            scroll_lines(editor, 1);
            true
        }
        KeyCode::Char('G') if key.modifiers.contains(KeyModifiers::SHIFT) => {
            editor.terminal.pending_scroll_top = false;
            scroll_to_bottom(editor);
            true
        }
        KeyCode::Char('g') => {
            if editor.terminal.pending_scroll_top {
                editor.terminal.pending_scroll_top = false;
                scroll_to_top(editor);
            } else {
                editor.terminal.pending_scroll_top = true;
            }
            true
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            editor.terminal.pending_scroll_top = false;
            scroll_half_page(editor, 1);
            true
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            editor.terminal.pending_scroll_top = false;
            scroll_half_page(editor, -1);
            true
        }
        _ => false,
    }
}

fn active_session_id(editor: &Editor) -> Option<String> {
    let panel_id = editor.tree.focus;
    if let Some(panel) = editor.tree.terminal_panel(panel_id) {
        return Some(panel.session_id.clone());
    }
    editor.terminal.active_session.clone()
}

fn scroll_lines(editor: &mut Editor, delta: i32) {
    let Some(session_id) = active_session_id(editor) else {
        return;
    };

    if delta < 0 {
        if let Some(session) = editor.terminal.session_mut(&session_id) {
            session.scroll_pinned = false;
        }
    }

    if let Some(handle) = crate::terminal::session_handle(&session_id) {
        handle.scroll(TerminalScroll::Delta(delta));
        if let Some(session) = editor.terminal.session_mut(&session_id) {
            session.scroll_offset = handle.display_offset();
        }
    }
    helix_event::request_redraw();
}

fn scroll_half_page(editor: &mut Editor, direction: i32) {
    let Some(session_id) = active_session_id(editor) else {
        return;
    };
    let area = editor
        .tree
        .terminal_panels()
        .find(|(panel, _)| panel.session_id == session_id)
        .map(|(panel, _)| panel.area)
        .unwrap_or_default();
    let (rows, _) = grid_size(area);
    let delta = (rows as i32 / 2).max(1) * direction;
    scroll_lines(editor, delta);
}

fn scroll_to_top(editor: &mut Editor) {
    let Some(session_id) = active_session_id(editor) else {
        return;
    };
    if let Some(session) = editor.terminal.session_mut(&session_id) {
        session.scroll_pinned = false;
    }
    if let Some(handle) = crate::terminal::session_handle(&session_id) {
        handle.scroll(TerminalScroll::Top);
        if let Some(session) = editor.terminal.session_mut(&session_id) {
            session.scroll_offset = handle.display_offset();
        }
    }
    helix_event::request_redraw();
}

fn scroll_to_bottom(editor: &mut Editor) {
    let Some(session_id) = active_session_id(editor) else {
        return;
    };
    if let Some(session) = editor.terminal.session_mut(&session_id) {
        session.scroll_pinned = true;
        session.scroll_offset = 0;
    }
    if let Some(handle) = crate::terminal::session_handle(&session_id) {
        handle.scroll(TerminalScroll::Bottom);
    }
    helix_event::request_redraw();
}

fn encode_key(key: KeyEvent) -> Vec<u8> {
    match key.code {
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            c.to_string().into_bytes()
        }
        KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => vec![0x0a],
        KeyCode::Char('m') if key.modifiers.contains(KeyModifiers::CONTROL) => vec![b'\r'],
        KeyCode::Up => vec![0x1b, b'[', b'A'],
        KeyCode::Down => vec![0x1b, b'[', b'B'],
        KeyCode::Right => vec![0x1b, b'[', b'C'],
        KeyCode::Left => vec![0x1b, b'[', b'D'],
        KeyCode::Home => vec![0x1b, b'[', b'H'],
        KeyCode::End => vec![0x1b, b'[', b'F'],
        KeyCode::PageUp => vec![0x1b, b'[', b'5', b'~'],
        KeyCode::PageDown => vec![0x1b, b'[', b'6', b'~'],
        KeyCode::Delete => vec![0x1b, b'[', b'3', b'~'],
        _ => Vec::new(),
    }
}

pub fn cursor(editor: &Editor, _area: Rect) -> (Option<Position>, CursorKind) {
    if !editor.terminal_panel_focused() {
        return (None, CursorKind::Hidden);
    }

    let Some(session_id) = active_session_id(editor) else {
        return (None, CursorKind::Hidden);
    };

    let Some(handle) = crate::terminal::session_handle(&session_id) else {
        return (None, CursorKind::Hidden);
    };

    let area = editor
        .tree
        .terminal_panels()
        .find(|(panel, _)| panel.session_id == session_id)
        .map(|(panel, _)| panel.area)
        .unwrap_or_default();
    let body = grid_area(area);
    if body.height == 0 || body.width == 0 {
        return (None, CursorKind::Hidden);
    }

    let (cursor_point, hidden, display_offset) = handle.with_term(|term| {
        let content = term.renderable_content();
        (
            content.cursor.point,
            matches!(
                content.cursor.shape,
                alacritty_terminal::vte::ansi::CursorShape::Hidden
            ),
            content.display_offset,
        )
    });

    if hidden {
        return (None, CursorKind::Hidden);
    }

    let viewport_top = -(display_offset as i32);
    let row = cursor_point.line.0 - viewport_top;
    if row < 0 || row as u16 >= body.height {
        return (None, CursorKind::Hidden);
    }

    let col = cursor_point.column.0;
    if col as u16 >= body.width {
        return (None, CursorKind::Hidden);
    }

    (
        Some(Position::new(
            (body.y + row as u16) as usize,
            (body.x + col as u16) as usize,
        )),
        CursorKind::Block,
    )
}

pub fn resize_panels(editor: &Editor) {
    for (panel, _) in editor.tree.terminal_panels() {
        let (rows, cols) = grid_size(panel.area);
        crate::terminal::send(helix_pty::TerminalCommand::Resize {
            id: panel.session_id.clone().into(),
            rows,
            cols,
        });
    }
}
