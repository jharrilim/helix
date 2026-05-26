use helix_core::doc_formatter::TextFormat;
use helix_core::text_annotations::LineAnnotation;
use helix_core::{softwrapped_dimensions, Position};

use crate::Document;
use helix_review::ReviewComment;

const MAX_WRAP: u16 = 20;

pub struct ReviewLineAnnotation<'a> {
    doc: &'a Document,
    width: u16,
}

impl<'a> ReviewLineAnnotation<'a> {
    pub fn new(doc: &'a Document, width: u16) -> Box<dyn LineAnnotation + 'a> {
        Box::new(ReviewLineAnnotation { doc, width })
    }

    fn text_fmt(&self) -> TextFormat {
        let prefix_len = 2;
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

    fn comments_on_line(&self, doc_line: usize) -> Vec<&ReviewComment> {
        self.doc
            .review_comments
            .iter()
            .filter(|comment| comment.line == doc_line)
            .collect()
    }

    fn comment_height(&self, comment: &ReviewComment) -> usize {
        let text_fmt = self.text_fmt();
        softwrapped_dimensions(comment.body.as_str().into(), &text_fmt).0
    }
}

impl LineAnnotation for ReviewLineAnnotation<'_> {
    fn insert_virtual_lines(
        &mut self,
        _line_end_char_idx: usize,
        _line_end_visual_pos: Position,
        doc_line: usize,
    ) -> Position {
        let height: usize = self
            .comments_on_line(doc_line)
            .iter()
            .map(|comment| self.comment_height(comment))
            .sum();
        Position::new(height, 0)
    }
}
