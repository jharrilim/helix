use helix_view::{graphics::Rect, graphics::Style, Theme};
use tui::widgets::{Block, Borders};

/// Panel borders excluding the bottom edge so the status/footer row aligns with editor views.
pub fn panel_borders() -> Borders {
    Borders::LEFT | Borders::TOP | Borders::RIGHT
}

pub fn panel_inner(area: Rect) -> Rect {
    Block::default().borders(panel_borders()).inner(area)
}

/// Border style for auxiliary panels, matching editor split borders.
pub fn border_style(theme: &Theme) -> Style {
    theme.get("ui.window")
}

/// Status bar style for auxiliary panels, matching the editor status line.
pub fn statusline_style(theme: &Theme, focused: bool) -> Style {
    if focused {
        theme.get("ui.statusline")
    } else {
        theme.get("ui.statusline.inactive")
    }
}
