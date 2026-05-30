use helix_core::text_annotations::LineAnnotation;
use helix_core::Position;
use crate::review::DRAFT_COMMENT_ID;
use crate::Document;
use helix_review::{comment_box_height, comment_box_width, ReviewComment};

pub struct ReviewLineAnnotation<'a> {
    doc: &'a Document,
    width: u16,
    reserved_for_line: Option<usize>,
}

impl<'a> ReviewLineAnnotation<'a> {
    pub fn new(doc: &'a Document, width: u16) -> Box<dyn LineAnnotation + 'a> {
        Box::new(ReviewLineAnnotation {
            doc,
            width: comment_box_width(width),
            reserved_for_line: None,
        })
    }

    fn comments_on_line(&self, doc_line: usize) -> Vec<&ReviewComment> {
        self.doc
            .review_comments
            .iter()
            .filter(|comment| comment.line == doc_line)
            .collect()
    }

    fn is_document_line_end(&self, doc_line: usize, line_end_char_idx: usize) -> bool {
        let text = self.doc.text();
        let line_end = text.line_to_char(doc_line + 1);
        line_end_char_idx + 1 >= line_end
    }
}

impl LineAnnotation for ReviewLineAnnotation<'_> {
    fn reset_pos(&mut self, _char_idx: usize) -> usize {
        self.reserved_for_line = None;
        usize::MAX
    }

    fn insert_virtual_lines(
        &mut self,
        line_end_char_idx: usize,
        _line_end_visual_pos: Position,
        doc_line: usize,
    ) -> Position {
        if !self.is_document_line_end(doc_line, line_end_char_idx) {
            return Position::new(0, 0);
        }
        if self.reserved_for_line == Some(doc_line) {
            return Position::new(0, 0);
        }
        self.reserved_for_line = Some(doc_line);

        let height: usize = self
            .comments_on_line(doc_line)
            .iter()
            .map(|comment| {
                let is_draft = comment.id == DRAFT_COMMENT_ID;
                comment_box_height(&comment.body, self.width, is_draft)
            })
            .sum();
        Position::new(height, 0)
    }
}
