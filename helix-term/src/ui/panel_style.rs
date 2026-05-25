use helix_view::{graphics::Style, Theme};

/// Border style for auxiliary panels, matching editor split borders.
pub fn border_style(theme: &Theme) -> Style {
    theme.get("ui.window")
}
