use helix_core::doc_formatter::TextFormat;
use helix_core::softwrapped_dimensions;

use crate::CommentAuthor;

/// Rows reserved for the metadata header (`┌ You · …`).
pub const HEADER_ROWS: usize = 1;

/// Rows reserved for the bottom border (`└ ─…`).
pub const FOOTER_ROWS: usize = 1;

/// Left gutter width (`│ `).
pub const GUTTER_WIDTH: u16 = 2;

/// Right border column (`│`).
pub const RIGHT_BORDER_WIDTH: u16 = 1;

/// Maximum width of a review comment box (including borders).
pub const MAX_BOX_WIDTH: u16 = 82;

/// Footer action shown while drafting a comment.
pub const SAVE_LABEL: &str = "Save";

/// Footer action that discards the draft comment.
pub const DELETE_LABEL: &str = "Delete";

/// Column positions for draft footer buttons (inside the left/right borders).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FooterButtonColumns {
    pub delete_col: u16,
    pub save_col: u16,
}

/// Place `Delete` and `Save` on the bottom row, aligned to the right inside the box.
pub fn footer_button_columns(box_width: u16) -> FooterButtonColumns {
    let save_len = SAVE_LABEL.chars().count() as u16;
    let delete_len = DELETE_LABEL.chars().count() as u16;
    let gap = 2u16;
    let right_inner = box_width.saturating_sub(2);
    let save_col = right_inner.saturating_sub(save_len);
    let delete_col = save_col.saturating_sub(gap + delete_len);
    FooterButtonColumns {
        delete_col,
        save_col,
    }
}

const MAX_WRAP: u16 = 20;

/// Effective box width for layout and rendering.
pub fn comment_box_width(viewport_width: u16) -> u16 {
    viewport_width.min(MAX_BOX_WIDTH)
}

/// Text layout for comment body inside the box gutter.
pub fn comment_text_format(viewport_width: u16) -> TextFormat {
    let width = comment_box_width(viewport_width)
        .saturating_sub(GUTTER_WIDTH)
        .saturating_sub(RIGHT_BORDER_WIDTH);
    TextFormat {
        soft_wrap: true,
        tab_width: 4,
        max_wrap: MAX_WRAP.min(width / 4),
        max_indent_retain: 0,
        wrap_indicator: "".into(),
        wrap_indicator_highlight: None,
        viewport_width: width,
        soft_wrap_at_text_width: true,
    }
}

/// Body text used for layout/rendering (placeholder while drafting empty comments).
pub fn comment_display_body<'a>(body: &'a str, is_draft: bool) -> &'a str {
    if body.is_empty() && is_draft {
        "…"
    } else {
        body
    }
}

/// Total virtual rows for a bordered comment box (header + wrapped body + footer).
pub fn comment_box_height(body: &str, viewport_width: u16, is_draft: bool) -> usize {
    let display = comment_display_body(body, is_draft);
    let text_fmt = comment_text_format(comment_box_width(viewport_width));
    let body_rows = softwrapped_dimensions(display.into(), &text_fmt).0;
    HEADER_ROWS + body_rows + FOOTER_ROWS
}

/// Human-readable label for an author.
pub fn author_label(author: CommentAuthor) -> &'static str {
    match author {
        CommentAuthor::User => "You",
        CommentAuthor::Agent => "Agent",
    }
}

/// Format a unix-seconds timestamp string for display in comment headers.
pub fn format_comment_timestamp(created_at: &str) -> String {
    let Ok(secs) = created_at.parse::<i64>() else {
        return created_at.to_string();
    };
    if secs <= 0 {
        return String::new();
    }
    chrono::DateTime::from_timestamp(secs, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| created_at.to_string())
}

/// Build the header string shown in the top row of a comment box.
pub fn comment_header_text(
    author: CommentAuthor,
    created_at: &str,
    is_draft: bool,
) -> String {
    let mut header = author_label(author).to_string();
    if is_draft {
        header.push_str(" · Draft");
    } else {
        let ts = format_comment_timestamp(created_at);
        if !ts.is_empty() {
            header.push_str(" · ");
            header.push_str(&ts);
        }
    }
    header
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comment_box_height_includes_header_and_footer() {
        let height = comment_box_height("hello", 80, false);
        assert_eq!(height, HEADER_ROWS + 1 + FOOTER_ROWS);
    }

    #[test]
    fn comment_box_height_multiline() {
        let height = comment_box_height("line one\nline two", 80, false);
        assert!(height > HEADER_ROWS + 1 + FOOTER_ROWS);
    }

    #[test]
    fn empty_draft_reserves_placeholder_row() {
        let height = comment_box_height("", 80, true);
        assert_eq!(height, HEADER_ROWS + 1 + FOOTER_ROWS);
    }

    #[test]
    fn comment_box_width_is_capped() {
        assert_eq!(comment_box_width(200), MAX_BOX_WIDTH);
        assert_eq!(comment_box_width(40), 40);
    }

    #[test]
    fn format_timestamp_parses_unix() {
        let formatted = format_comment_timestamp("1748187082");
        assert!(formatted.contains('-'));
    }

    #[test]
    fn draft_header_omits_timestamp() {
        assert_eq!(
            comment_header_text(CommentAuthor::User, "", true),
            "You · Draft"
        );
    }

    #[test]
    fn footer_buttons_fit_inside_box() {
        let cols = footer_button_columns(40);
        assert!(cols.delete_col > 0);
        assert!(cols.save_col > cols.delete_col);
        assert!(cols.save_col + (SAVE_LABEL.chars().count() as u16) < 40);
    }
}
