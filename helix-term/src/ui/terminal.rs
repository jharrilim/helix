use alacritty_terminal::grid::Dimensions;
use crate::compositor::EventResult;
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::color::Colors;
use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};
use helix_core::unicode::width::UnicodeWidthChar;
use helix_core::Position;
use helix_pty::{GridPoint, SelectionKind, TerminalScroll, viewport_point_to_grid};
use helix_view::{
    graphics::{Color as HelixColor, CursorKind, Modifier, Rect, Style},
    input::{KeyEvent, MouseButton, MouseEvent, MouseEventKind},
    keyboard::{KeyCode, KeyModifiers},
    terminal::{
        TerminalFocus, TerminalGridPoint, TerminalSearchMatch, TerminalSelection,
        TerminalSelectionKind,
    },
    Editor, ViewId,
};
use tui::buffer::Buffer as Surface;
use tui::text::Span;
use tui::widgets::{Block, Borders, Widget};

const TAB_BAR_HEIGHT: u16 = 1;
const HEADER_HEIGHT: u16 = 1;

pub fn panel_inner(area: Rect) -> Rect {
    Block::default().borders(Borders::ALL).inner(area)
}

pub fn tab_bar_area(area: Rect) -> Rect {
    let inner = panel_inner(area);
    if inner.height == 0 {
        return Rect::default();
    }
    Rect {
        height: TAB_BAR_HEIGHT.min(inner.height),
        ..inner
    }
}

pub fn status_header_area(area: Rect) -> Rect {
    let inner = panel_inner(area);
    let tab = tab_bar_area(area);
    if inner.height <= tab.height {
        return Rect::default();
    }
    Rect {
        y: tab.bottom(),
        height: HEADER_HEIGHT.min(inner.height.saturating_sub(tab.height)),
        width: inner.width,
        x: inner.x,
    }
}

pub fn grid_area(area: Rect) -> Rect {
    let inner = panel_inner(area);
    let top = tab_bar_area(area).height + status_header_area(area).height;
    if inner.height <= top {
        return Rect::default();
    }
    Rect {
        y: inner.y + top,
        height: inner.height.saturating_sub(top),
        ..inner
    }
}

fn to_grid(point: TerminalGridPoint) -> GridPoint {
    GridPoint {
        line: point.line,
        col: point.col,
    }
}

fn to_terminal(point: GridPoint) -> TerminalGridPoint {
    TerminalGridPoint {
        line: point.line,
        col: point.col,
    }
}

fn selection_kind(kind: TerminalSelectionKind) -> SelectionKind {
    match kind {
        TerminalSelectionKind::Char => SelectionKind::Char,
        TerminalSelectionKind::Line => SelectionKind::Line,
    }
}

/// Returns the terminal panel under the given screen coordinates, if any.
pub fn panel_at_coords(editor: &Editor, row: u16, column: u16) -> Option<ViewId> {
    editor.tree.terminal_panels().find_map(|(panel, _)| {
        contains_coords(panel.area, row, column).then_some(panel.id)
    })
}

fn contains_coords(area: Rect, row: u16, column: u16) -> bool {
    row >= area.y && row < area.bottom() && column >= area.x && column < area.right()
}

pub fn grid_size(area: Rect) -> (u16, u16) {
    let grid = grid_area(area);
    (grid.height.max(1), grid.width.max(2))
}

fn tab_label_for(editor: &Editor, session_id: &str) -> String {
    editor
        .terminal
        .sessions
        .get(session_id)
        .map(|session| format!(" {} ", session.tab_label()))
        .unwrap_or_else(|| format!(" {session_id} "))
}

pub fn render_tab_bar(editor: &Editor, area: Rect, surface: &mut Surface) {
    let tab_area = tab_bar_area(area);
    if tab_area.height == 0 || tab_area.width == 0 {
        return;
    }

    let active_style = editor
        .theme
        .try_get("ui.bufferline.active")
        .unwrap_or_else(|| editor.theme.get("ui.selection.active"));
    let inactive_style = editor
        .theme
        .try_get("ui.bufferline")
        .unwrap_or_else(|| editor.theme.get("ui.selection"));
    let background = editor
        .theme
        .try_get("ui.bufferline.background")
        .unwrap_or_else(|| editor.theme.get("ui.background"));

    surface.clear_with(tab_area, background);

    let active = editor.terminal.active_session.as_deref();
    let mut x = tab_area.x;
    for session_id in &editor.terminal.session_order {
        let label = tab_label_for(editor, session_id);
        let style = if active == Some(session_id.as_str()) {
            active_style
        } else {
            inactive_style
        };
        let rem = tab_area.right().saturating_sub(x);
        if rem == 0 {
            break;
        }
        x = surface
            .set_stringn(x, tab_area.y, &label, rem as usize, style)
            .0;
    }
}

fn session_id_at_tab(editor: &Editor, area: Rect, row: u16, column: u16) -> Option<String> {
    let tab_area = tab_bar_area(area);
    if !contains_coords(tab_area, row, column) {
        return None;
    }
    let active = editor.terminal.active_session.as_deref();
    let mut x = tab_area.x;
    for session_id in &editor.terminal.session_order {
        let label = tab_label_for(editor, session_id);
        let width = label.len() as u16;
        if column >= x && column < x + width {
            return Some(session_id.clone());
        }
        x += width;
        if x >= tab_area.right() {
            break;
        }
        let _ = active;
    }
    None
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
    let selection_style = theme.get("ui.selection");
    let search_style = theme.get("ui.highlight");
    let search_current_style = theme.get("ui.selection.active");

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

    let mode = if focused {
        match editor.terminal.focus {
            TerminalFocus::Insert => "I",
            TerminalFocus::Select => "S",
            TerminalFocus::Normal => "N",
        }
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

    render_tab_bar(editor, area, surface);

    let header_area = status_header_area(area);
    let body = grid_area(area);

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
    let selection = editor.terminal.selection;
    let search = editor.terminal.search.as_ref();

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

            let grid_point = to_terminal(viewport_point_to_grid(term, row, col));
            let mut style = cell_to_style(cell, &content.colors, default_fg, default_bg);

            if let Some(search) = search {
                if search
                    .matches
                    .iter()
                    .enumerate()
                    .any(|(idx, m)| {
                        m.line == grid_point.line
                            && col >= m.col
                            && col < m.col + search.pattern.len()
                            && idx == search.current
                    })
                {
                    style = style.patch(search_current_style);
                } else if search.matches.iter().any(|m| {
                    m.line == grid_point.line
                        && col >= m.col
                        && col < m.col + search.pattern.len()
                }) {
                    style = style.patch(search_style);
                }
            }

            if let Some(sel) = selection {
                if point_in_selection(grid_point, sel) {
                    style = style.patch(selection_style);
                }
            }

            let c = cell.c;
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

fn point_in_selection(point: TerminalGridPoint, sel: TerminalSelection) -> bool {
    let (start, end) = sel.normalized();
    match sel.kind {
        TerminalSelectionKind::Line => point.line >= start.line && point.line <= end.line,
        TerminalSelectionKind::Char => {
            if point.line < start.line || point.line > end.line {
                false
            } else if start.line == end.line {
                point.col >= start.col && point.col <= end.col
            } else if point.line == start.line {
                point.col >= start.col
            } else if point.line == end.line {
                point.col <= end.col
            } else {
                true
            }
        }
    }
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

    let panel = editor.tree.terminal_panel(panel_id).unwrap();
    let tab_area = tab_bar_area(panel.area);
    let header = status_header_area(panel.area);
    let body = grid_area(panel.area);

    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(session_id) = session_id_at_tab(editor, panel.area, event.row, event.column)
            {
                editor.switch_terminal_session(&session_id);
            } else if contains_coords(header, event.row, event.column) {
                editor.terminal.focus = TerminalFocus::Normal;
                editor.mode = helix_view::document::Mode::Normal;
            } else if contains_coords(tab_area, event.row, event.column) {
                editor.terminal.focus = TerminalFocus::Normal;
                editor.mode = helix_view::document::Mode::Normal;
            } else if contains_coords(body, event.row, event.column) {
                editor.terminal.focus = TerminalFocus::Normal;
                editor.mode = helix_view::document::Mode::Normal;
                if let Some(point) = point_from_mouse(editor, body, event.row, event.column) {
                    editor.terminal.selection = Some(TerminalSelection::new(
                        point,
                        TerminalSelectionKind::Char,
                    ));
                }
            }
            helix_event::request_redraw();
            EventResult::Consumed(None)
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if contains_coords(body, event.row, event.column) {
                if let Some(point) = point_from_mouse(editor, body, event.row, event.column) {
                    if let Some(sel) = editor.terminal.selection.as_mut() {
                        sel.head = point;
                        sel.dragging = true;
                    } else {
                        editor.terminal.selection = Some(TerminalSelection::new(
                            point,
                            TerminalSelectionKind::Char,
                        ));
                    }
                }
                helix_event::request_redraw();
            }
            EventResult::Consumed(None)
        }
        MouseEventKind::Up(MouseButton::Left) => {
            let mut yanked = false;
            if let Some(sel) = editor.terminal.selection.as_mut() {
                sel.dragging = false;
                if yank_selection(editor) {
                    yanked = true;
                }
            }
            if yanked {
                helix_event::request_redraw();
            }
            EventResult::Consumed(None)
        }
        MouseEventKind::ScrollUp if contains_coords(body, event.row, event.column) => {
            scroll_lines(editor, 1);
            EventResult::Consumed(None)
        }
        MouseEventKind::ScrollDown if contains_coords(body, event.row, event.column) => {
            scroll_lines(editor, -1);
            EventResult::Consumed(None)
        }
        _ => EventResult::Ignored(None),
    }
}

fn point_from_mouse(
    editor: &Editor,
    body: Rect,
    row: u16,
    column: u16,
) -> Option<TerminalGridPoint> {
    if body.height == 0 || body.width == 0 {
        return None;
    }
    let viewport_row = (row.saturating_sub(body.y)) as usize;
    let col = (column.saturating_sub(body.x)) as usize;
    if viewport_row >= body.height as usize || col >= body.width as usize {
        return None;
    }
    let session_id = active_session_id(editor)?;
    let handle = crate::terminal::session_handle(&session_id)?;
    Some(to_terminal(handle.viewport_point_to_grid(viewport_row, col)))
}

pub fn paste_to_terminal(editor: &mut Editor, contents: &str) {
    let Some(session_id) = active_session_id(editor) else {
        return;
    };
    if contents.is_empty() {
        return;
    }
    crate::terminal::send(helix_pty::TerminalCommand::Write {
        id: session_id.into(),
        data: contents.as_bytes().to_vec(),
    });
}

pub fn handle_key(editor: &mut Editor, key: KeyEvent) -> bool {
    if !editor.terminal.is_open() {
        return false;
    }

    if editor.terminal.register_pending {
        editor.terminal.register_pending = false;
        let register = match key.code {
            KeyCode::Char(c) => c,
            KeyCode::Esc => return true,
            _ => return true,
        };
        let pasted = editor
            .registers
            .read(register, editor)
            .and_then(|mut values| values.next().map(|text| text.into_owned()));
        if let Some(text) = pasted {
            paste_to_terminal(editor, &text);
        }
        return true;
    }

    if key.code == KeyCode::Char('"') && !key.modifiers.contains(KeyModifiers::CONTROL) {
        editor.terminal.register_pending = true;
        return true;
    }

    if key.code == KeyCode::Esc {
        editor.terminal.focus = TerminalFocus::Normal;
        editor.terminal.clear_selection();
        editor.terminal.clear_search();
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

pub fn handle_normal_key_with_search(editor: &mut Editor, key: KeyEvent) -> bool {
    if key.code == KeyCode::Char('/') && !key.modifiers.contains(KeyModifiers::CONTROL) {
        editor.terminal.open_search_prompt = true;
        return true;
    }
    if key.code == KeyCode::Char('t') && !key.modifiers.contains(KeyModifiers::SHIFT) {
        if editor.terminal.tab_menu_active {
            crate::ui::terminal_tabs::close(editor);
        } else {
            crate::ui::terminal_tabs::open(editor);
        }
        return true;
    }
    handle_normal_key_impl(editor, key, true)
}

fn handle_normal_key_impl(editor: &mut Editor, key: KeyEvent, allow_search_nav: bool) -> bool {
    if !editor.terminal.is_open() {
        return false;
    }

    if allow_search_nav {
        if key.code == KeyCode::Char('n') && !key.modifiers.contains(KeyModifiers::SHIFT) {
            if search_next(editor, false) {
                return true;
            }
        }
        if key.code == KeyCode::Char('N') && key.modifiers.contains(KeyModifiers::SHIFT) {
            if search_next(editor, true) {
                return true;
            }
        }
    }

    if editor.terminal.focus == TerminalFocus::Select {
        return handle_select_key(editor, key);
    }

    match key.code {
        KeyCode::Char('v') if !key.modifiers.contains(KeyModifiers::SHIFT) => {
            enter_select_mode(editor, TerminalSelectionKind::Char);
            true
        }
        KeyCode::Char('V') if key.modifiers.contains(KeyModifiers::SHIFT) => {
            enter_select_mode(editor, TerminalSelectionKind::Line);
            true
        }
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
        KeyCode::Esc => {
            editor.terminal.clear_selection();
            editor.terminal.clear_search();
            helix_event::request_redraw();
            true
        }
        _ => false,
    }
}

fn handle_select_key(editor: &mut Editor, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('y') => yank_selection(editor),
        KeyCode::Esc => {
            editor.terminal.focus = TerminalFocus::Normal;
            editor.terminal.clear_selection();
            editor.mode = helix_view::document::Mode::Normal;
            helix_event::request_redraw();
            true
        }
        KeyCode::Char('v') if !key.modifiers.contains(KeyModifiers::SHIFT) => {
            if let Some(sel) = editor.terminal.selection.as_mut() {
                sel.kind = TerminalSelectionKind::Char;
            }
            true
        }
        KeyCode::Char('V') if key.modifiers.contains(KeyModifiers::SHIFT) => {
            if let Some(sel) = editor.terminal.selection.as_mut() {
                sel.kind = TerminalSelectionKind::Line;
            }
            true
        }
        KeyCode::Char('j') => {
            move_selection_head(editor, 1, 0);
            true
        }
        KeyCode::Char('k') => {
            move_selection_head(editor, -1, 0);
            true
        }
        KeyCode::Char('h') => {
            move_selection_head(editor, 0, -1);
            true
        }
        KeyCode::Char('l') => {
            move_selection_head(editor, 0, 1);
            true
        }
        KeyCode::Char('G') if key.modifiers.contains(KeyModifiers::SHIFT) => {
            scroll_to_bottom(editor);
            if let Some(point) = cursor_grid_point(editor) {
                if let Some(sel) = editor.terminal.selection.as_mut() {
                    sel.head = point;
                }
            }
            true
        }
        KeyCode::Char('g') => {
            scroll_to_top(editor);
            if let Some(point) = top_grid_point(editor) {
                if let Some(sel) = editor.terminal.selection.as_mut() {
                    sel.head = point;
                }
            }
            true
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            scroll_half_page(editor, 1);
            true
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            scroll_half_page(editor, -1);
            true
        }
        _ => false,
    }
}

fn enter_select_mode(editor: &mut Editor, kind: TerminalSelectionKind) {
    editor.terminal.pending_scroll_top = false;
    editor.terminal.focus = TerminalFocus::Select;
    editor.mode = helix_view::document::Mode::Normal;
    let point = cursor_grid_point(editor).unwrap_or_default();
    editor.terminal.selection = Some(TerminalSelection::new(point, kind));
    helix_event::request_redraw();
}

fn move_selection_head(editor: &mut Editor, line_delta: i32, col_delta: i32) {
    let Some(session_id) = active_session_id(editor) else {
        return;
    };
    let Some(handle) = crate::terminal::session_handle(&session_id) else {
        return;
    };

    let cols = handle.with_term(|term| term.columns());
    if let Some(sel) = editor.terminal.selection.as_mut() {
        let mut head = sel.head;
        head.line = head.line.saturating_add(line_delta);
        if col_delta < 0 {
            head.col = head.col.saturating_sub((-col_delta) as usize);
        } else {
            head.col = (head.col + col_delta as usize).min(cols.saturating_sub(1));
        }
        sel.head = head;
    }

    if line_delta > 0 {
        scroll_lines(editor, -1);
    } else if line_delta < 0 {
        scroll_lines(editor, 1);
    }

    helix_event::request_redraw();
}

fn cursor_grid_point(editor: &Editor) -> Option<TerminalGridPoint> {
    let session_id = active_session_id(editor)?;
    let handle = crate::terminal::session_handle(&session_id)?;
    handle.with_term(|term| {
        let point = term.renderable_content().cursor.point;
        Some(TerminalGridPoint {
            line: point.line.0,
            col: point.column.0,
        })
    })
}

fn top_grid_point(editor: &Editor) -> Option<TerminalGridPoint> {
    let session_id = active_session_id(editor)?;
    let handle = crate::terminal::session_handle(&session_id)?;
    let display_offset = handle.display_offset();
    Some(TerminalGridPoint {
        line: -(display_offset as i32),
        col: 0,
    })
}

fn yank_selection(editor: &mut Editor) -> bool {
    let Some(session_id) = active_session_id(editor) else {
        return false;
    };
    let Some(handle) = crate::terminal::session_handle(&session_id) else {
        return false;
    };
    let Some(sel) = editor.terminal.selection else {
        return false;
    };
    let text = handle.range_text(
        to_grid(sel.anchor),
        to_grid(sel.head),
        selection_kind(sel.kind),
    );
    if text.is_empty() {
        return false;
    }

    let config = editor.config();
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
                format!("yanked terminal selection to register {mouse_register}")
            } else {
                format!(
                    "yanked terminal selection to registers {mouse_register} and {clipboard_register}"
                )
            };
            editor.set_status(status);
        }
        (Err(err), _) | (_, Err(err)) => editor.set_error(err.to_string()),
    }
    true
}

pub fn open_search_prompt(cx: &mut crate::commands::Context) {
    use crate::ui::PromptEvent;
    use helix_stdx::rope::RopeSliceExt;
    use helix_core::ropey::Rope;

    crate::ui::raw_regex_prompt(
        cx,
        "terminal:".into(),
        Some('/'),
        crate::ui::completers::none,
        move |cx, regex, input, event| {
            if event == PromptEvent::Abort {
                cx.editor.terminal.clear_search();
                return;
            }
            if event != PromptEvent::Validate {
                return;
            }

            let Some(session_id) = active_session_id(cx.editor) else {
                cx.editor.set_error("no active terminal session");
                return;
            };
            let Some(handle) = crate::terminal::session_handle(&session_id) else {
                cx.editor.set_error("terminal session not ready");
                return;
            };

            let pattern = input.to_string();
            let mut matches = Vec::new();
            for (line, text) in handle.all_lines() {
                let rope = Rope::from_str(&text);
                for mat in regex.find_iter(rope.slice(..).regex_input()) {
                    matches.push(TerminalSearchMatch {
                        line,
                        col: mat.start(),
                    });
                }
            }

            if matches.is_empty() {
                cx.editor.set_error("pattern not found");
                cx.editor.terminal.clear_search();
                return;
            }

            cx.editor.terminal.search = Some(helix_view::terminal::TerminalSearch {
                pattern,
                matches,
                current: 0,
            });
            jump_to_search_match(cx.editor, 0);
        },
    );
}

fn search_next(editor: &mut Editor, reverse: bool) -> bool {
    let index = {
        let Some(search) = editor.terminal.search.as_mut() else {
            return false;
        };
        if search.matches.is_empty() {
            return false;
        }
        let len = search.matches.len();
        search.current = if reverse {
            (search.current + len - 1) % len
        } else {
            (search.current + 1) % len
        };
        search.current
    };
    jump_to_search_match(editor, index);
    true
}

fn jump_to_search_match(editor: &mut Editor, index: usize) {
    let match_line = editor
        .terminal
        .search
        .as_ref()
        .and_then(|search| search.matches.get(index))
        .map(|m| m.line);
    let Some(match_line) = match_line else {
        return;
    };
    let Some(session_id) = active_session_id(editor) else {
        return;
    };
    let Some(handle) = crate::terminal::session_handle(&session_id) else {
        return;
    };

    if let Some(session) = editor.terminal.session_mut(&session_id) {
        session.scroll_pinned = false;
    }
    let current_offset = handle.display_offset();
    let target_offset = (-match_line).max(0) as usize;
    let delta = target_offset as i32 - current_offset as i32;
    if delta != 0 {
        handle.scroll(TerminalScroll::Delta(delta));
        if let Some(session) = editor.terminal.session_mut(&session_id) {
            session.scroll_offset = handle.display_offset();
        }
    }

    helix_event::request_redraw();
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
    let Some(panel_id) = editor.terminal.panel_id else {
        return;
    };
    let Some(panel) = editor.tree.terminal_panel(panel_id) else {
        return;
    };
    let (rows, cols) = grid_size(panel.area);
    if let Some(session_id) = editor.terminal.active_session.as_ref() {
        crate::terminal::send(helix_pty::TerminalCommand::Resize {
            id: session_id.clone().into(),
            rows,
            cols,
        });
    }
}
