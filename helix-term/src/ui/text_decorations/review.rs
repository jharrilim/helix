use helix_core::doc_formatter::{DocumentFormatter, TextFormat};
use helix_core::graphemes::{Grapheme, GraphemeStr};
use helix_core::unicode::segmentation::UnicodeSegmentation;
use helix_core::unicode::width::UnicodeWidthStr;
use helix_core::softwrapped_dimensions;
use helix_core::text_annotations::TextAnnotations;
use helix_core::Position;
use helix_review::{
    comment_box_height, comment_box_width, comment_display_body, comment_header_text,
    comment_text_format, footer_button_columns, DELETE_LABEL, GUTTER_WIDTH, SAVE_LABEL,
};
use helix_view::review::{DRAFT_COMMENT_ID, ReviewDraftButton};
use helix_view::{Document, Theme};

use helix_view::graphics::Modifier;

use crate::ui::document::{LinePos, TextRenderer};
use crate::ui::text_decorations::Decoration;

const TL_CORNER: &str = "┌";
const TR_CORNER: &str = "┐";
const BL_CORNER: &str = "└";
const BR_CORNER: &str = "┘";
const HOR_BAR: &str = "─";
const VER_BAR: &str = "│";

pub struct ReviewDecoration<'a> {
    doc: &'a Document,
    width: u16,
    style: helix_view::theme::Style,
    draft_style: helix_view::theme::Style,
    header_style: helix_view::theme::Style,
    button_style: helix_view::theme::Style,
}

impl<'a> ReviewDecoration<'a> {
    pub fn new(doc: &'a Document, theme: &Theme, width: u16) -> Self {
        let style = theme
            .try_get("ui.review.comment")
            .unwrap_or_else(|| theme.get("hint"));
        let draft_style = theme
            .try_get("ui.review.draft")
            .unwrap_or_else(|| theme.get("ui.text"));
        let header_style = theme
            .try_get("ui.review.header")
            .unwrap_or_else(|| style.add_modifier(Modifier::DIM));
        let button_style = theme
            .try_get("ui.review.button")
            .unwrap_or_else(|| draft_style);
        ReviewDecoration {
            doc,
            width: comment_box_width(width),
            style,
            draft_style,
            header_style,
            button_style,
        }
    }

    fn draw_label(
        &self,
        renderer: &mut TextRenderer,
        label: &str,
        row: u16,
        mut col: u16,
        style: helix_view::theme::Style,
        right_col: u16,
    ) {
        for g in label.graphemes(true) {
            if col >= right_col {
                break;
            }
            renderer.draw_decoration_grapheme(
                Grapheme::Other {
                    g: GraphemeStr::from(g),
                },
                style,
                row,
                col,
            );
            col += g.width() as u16;
        }
    }

    fn text_fmt(&self) -> TextFormat {
        comment_text_format(self.width)
    }

    fn comments_on_line(&self, doc_line: usize) -> Vec<&helix_view::ReviewComment> {
        self.doc
            .review_comments()
            .iter()
            .filter(|comment| comment.line == doc_line)
            .collect()
    }

    fn right_col(&self) -> u16 {
        self.width.saturating_sub(1)
    }

    fn fill_horizontal(
        &self,
        renderer: &mut TextRenderer,
        row: u16,
        start_col: u16,
        style: helix_view::theme::Style,
    ) {
        let end = self.right_col().saturating_sub(1);
        if start_col > end {
            return;
        }
        for col in start_col..=end {
            renderer.draw_decoration_grapheme(
                Grapheme::new_decoration(HOR_BAR),
                style,
                row,
                col,
            );
        }
    }

    fn draft_cursor_char_idx(body: &str, cursor_byte: usize) -> usize {
        if body.is_empty() {
            return 0;
        }
        let mut end = cursor_byte.min(body.len());
        while end > 0 && !body.is_char_boundary(end) {
            end -= 1;
        }
        body[..end].chars().count()
    }

    /// Match the prompt caret: before `cursor_char`, on the grapheme at that index;
    /// at end-of-text, on the formatter's EOF cell (the trailing space).
    fn grapheme_at_cursor(
        grapheme: &helix_core::doc_formatter::FormattedGrapheme<'_>,
        cursor_char: usize,
    ) -> bool {
        if grapheme.source.is_eof() {
            return grapheme.char_idx == cursor_char;
        }
        grapheme.char_idx == cursor_char
    }

    fn draw_comment(
        &self,
        renderer: &mut TextRenderer,
        comment: &helix_view::ReviewComment,
        row: u16,
    ) {
        let is_draft = comment.id == DRAFT_COMMENT_ID;
        let body_style = if is_draft {
            self.draft_style
        } else {
            self.style
        };
        let header_style = self.header_style.patch(body_style);

        let header = comment_header_text(comment.author, &comment.created_at, is_draft);

        let right_col = self.right_col();

        // Header row (use decoration graphemes — same coordinates as border drawing)
        renderer.draw_decoration_grapheme(
            Grapheme::new_decoration(TL_CORNER),
            body_style,
            row,
            0,
        );
        let mut header_col = 1u16;
        for g in header.graphemes(true) {
            if header_col >= right_col {
                break;
            }
            renderer.draw_decoration_grapheme(
                Grapheme::Other {
                    g: GraphemeStr::from(g),
                },
                header_style,
                row,
                header_col,
            );
            header_col += g.width() as u16;
        }
        self.fill_horizontal(renderer, row, header_col, body_style);
        renderer.draw_decoration_grapheme(
            Grapheme::new_decoration(TR_CORNER),
            body_style,
            row,
            right_col,
        );

        let display_body = comment_display_body(&comment.body, is_draft);
        let text_col = GUTTER_WIDTH;
        let text_fmt = self.text_fmt();
        let (body_line_count, _) = softwrapped_dimensions(display_body.into(), &text_fmt);
        let draft_cursor = is_draft.then(|| self.doc.review_draft_cursor()).flatten();
        let cursor_char = draft_cursor.map(|byte| Self::draft_cursor_char_idx(&comment.body, byte));

        for rel in 0..body_line_count {
            let body_row = row + 1 + rel as u16;
            renderer.draw_decoration_grapheme(
                Grapheme::new_decoration(VER_BAR),
                body_style,
                body_row,
                0,
            );
            renderer.draw_decoration_grapheme(
                Grapheme::new_decoration(VER_BAR),
                body_style,
                body_row,
                right_col,
            );
        }

        let annotations = TextAnnotations::default();
        let formatter = DocumentFormatter::new_at_prev_checkpoint(
            display_body.into(),
            &text_fmt,
            &annotations,
            0,
        );
        for grapheme in formatter {
            let body_row = row + 1 + grapheme.visual_pos.row as u16;
            let mut style = body_style;
            if let Some(cursor_char) = cursor_char {
                if Self::grapheme_at_cursor(&grapheme, cursor_char) {
                    style = style.add_modifier(Modifier::REVERSED);
                }
            }
            renderer.draw_decoration_grapheme(
                grapheme.raw,
                style,
                body_row,
                text_col + grapheme.visual_pos.col as u16,
            );
        }

        // Footer row (below all body lines; never overlaps comment text)
        let footer_row = row + 1 + body_line_count as u16;
        renderer.draw_decoration_grapheme(
            Grapheme::new_decoration(BL_CORNER),
            body_style,
            footer_row,
            0,
        );
        if is_draft {
            let cols = footer_button_columns(self.width);
            let focused = self.doc.review_draft_button();
            let delete_style = if focused == Some(ReviewDraftButton::Delete) {
                self.button_style.add_modifier(Modifier::REVERSED)
            } else {
                self.button_style
            };
            let save_style = if focused == Some(ReviewDraftButton::Save) {
                self.button_style.add_modifier(Modifier::REVERSED)
            } else {
                self.button_style
            };
            let bar_end = cols.delete_col.saturating_sub(1).max(1);
            for col in 1..=bar_end.min(self.right_col().saturating_sub(1)) {
                renderer.draw_decoration_grapheme(
                    Grapheme::new_decoration(HOR_BAR),
                    body_style,
                    footer_row,
                    col,
                );
            }
            self.draw_label(
                renderer,
                DELETE_LABEL,
                footer_row,
                cols.delete_col,
                delete_style,
                right_col,
            );
            self.draw_label(
                renderer,
                SAVE_LABEL,
                footer_row,
                cols.save_col,
                save_style,
                right_col,
            );
        } else {
            self.fill_horizontal(renderer, footer_row, 1, body_style);
        }
        renderer.draw_decoration_grapheme(
            Grapheme::new_decoration(BR_CORNER),
            body_style,
            footer_row,
            right_col,
        );
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
            let is_draft = comment.id == DRAFT_COMMENT_ID;
            rows_used += comment_box_height(&comment.body, self.width, is_draft);
        }
        Position::new(rows_used - virt_off.row, 0)
    }
}
