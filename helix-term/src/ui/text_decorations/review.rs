use helix_core::doc_formatter::{DocumentFormatter, TextFormat};
use helix_core::text_annotations::TextAnnotations;
use helix_core::{softwrapped_dimensions, Position};
use helix_view::review::DRAFT_COMMENT_ID;
use helix_view::{Document, Theme};

use crate::ui::document::{LinePos, TextRenderer};
use crate::ui::text_decorations::Decoration;

const PREFIX: &str = "│ ";
const MAX_WRAP: u16 = 20;

pub struct ReviewDecoration<'a> {
    doc: &'a Document,
    width: u16,
    style: helix_view::theme::Style,
    draft_style: helix_view::theme::Style,
}

impl<'a> ReviewDecoration<'a> {
    pub fn new(doc: &'a Document, theme: &Theme, width: u16) -> Self {
        let style = theme
            .try_get("ui.review.comment")
            .unwrap_or_else(|| theme.get("hint"));
        let draft_style = theme
            .try_get("ui.review.draft")
            .unwrap_or_else(|| theme.get("ui.text"));
        ReviewDecoration {
            doc,
            width,
            style,
            draft_style,
        }
    }

    fn text_fmt(&self) -> TextFormat {
        let prefix_len = PREFIX.len() as u16;
        let width = self.width.saturating_sub(prefix_len);
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

    fn comments_on_line(&self, doc_line: usize) -> Vec<&helix_view::ReviewComment> {
        self.doc
            .review_comments()
            .iter()
            .filter(|comment| comment.line == doc_line)
            .collect()
    }

    fn draw_comment(
        &self,
        renderer: &mut TextRenderer,
        comment: &helix_view::ReviewComment,
        row: u16,
    ) {
        let style = if comment.id == DRAFT_COMMENT_ID {
            self.draft_style
        } else {
            self.style
        };
        let text_fmt = self.text_fmt();
        let prefix_len = PREFIX.len() as u16;
        let text_col = prefix_len;
        let (height, _) = softwrapped_dimensions(comment.body.as_str().into(), &text_fmt);

        for line in 0..height {
            renderer.set_string(row + line as u16, 0, PREFIX, style);
        }

        let annotations = TextAnnotations::default();
        let formatter = DocumentFormatter::new_at_prev_checkpoint(
            comment.body.as_str().into(),
            &text_fmt,
            &annotations,
            0,
        );
        for grapheme in formatter {
            renderer.draw_decoration_grapheme(
                grapheme.raw,
                style,
                row + grapheme.visual_pos.row as u16,
                text_col + grapheme.visual_pos.col as u16,
            );
        }
    }
}

impl Decoration for ReviewDecoration<'_> {
    fn render_virt_lines(
        &mut self,
        renderer: &mut TextRenderer,
        pos: LinePos,
        virt_off: Position,
    ) -> Position {
        let comments = self.comments_on_line(pos.doc_line);
        let mut rows_used = virt_off.row;
        for comment in comments {
            let row = pos.visual_line + rows_used as u16;
            self.draw_comment(renderer, comment, row);
            let text_fmt = self.text_fmt();
            let (height, _) = softwrapped_dimensions(comment.body.as_str().into(), &text_fmt);
            rows_used += height;
        }
        Position::new(rows_used - virt_off.row, 0)
    }
}
