//! Code folding: fold ranges from tree-sitter and per-view collapsed state.

use helix_stdx::rope::RopeSliceExt as _;
use tree_house::{query_iter::QueryIterEvent, tree_sitter::Query};

use crate::{syntax::Syntax, RopeSlice};

/// A foldable region in the document. The header line (`start_line`) stays visible when folded;
/// lines `start_line + 1..=end_line` are hidden.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FoldRange {
    pub start_line: usize,
    pub end_line: usize,
}

impl FoldRange {
    pub fn new(start_line: usize, end_line: usize) -> Option<Self> {
        if end_line > start_line {
            Some(Self {
                start_line,
                end_line,
            })
        } else {
            None
        }
    }

    pub fn from_byte_range(text: RopeSlice, byte_range: std::ops::Range<u32>) -> Option<Self> {
        let start = text.byte_to_char(text.floor_char_boundary(byte_range.start as usize));
        let end = text.byte_to_char(text.ceil_char_boundary(byte_range.end as usize));
        let start_line = text.char_to_line(start);
        let end_line = text.char_to_line(end.saturating_sub(1));
        Self::new(start_line, end_line)
    }

    /// Returns true if `line` is hidden when this fold is collapsed.
    pub fn hides_line(&self, line: usize) -> bool {
        line > self.start_line && line <= self.end_line
    }

    pub fn contains_line(&self, line: usize) -> bool {
        line >= self.start_line && line <= self.end_line
    }

    pub fn intersects_lines(&self, start: usize, end: usize) -> bool {
        self.start_line <= end && self.end_line >= start
    }
}

/// Compiled tree-sitter `folds.scm` query.
#[derive(Debug)]
pub struct FoldQuery {
    pub query: Query,
    fold_capture: Option<tree_house::tree_sitter::Capture>,
}

impl FoldQuery {
    pub fn new(query: Query) -> Self {
        let fold_capture = query.get_capture("fold");
        Self {
            query,
            fold_capture,
        }
    }
}

/// Per-view collapsed fold state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FoldState {
    collapsed: Vec<FoldRange>,
}

impl FoldState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.collapsed.is_empty()
    }

    pub fn collapsed(&self) -> &[FoldRange] {
        &self.collapsed
    }

    pub fn clear(&mut self) {
        self.collapsed.clear();
    }

    pub fn is_line_hidden(&self, line: usize) -> bool {
        self.collapsed.iter().any(|f| f.hides_line(line))
    }

    pub fn is_collapsed_header(&self, line: usize) -> bool {
        self.collapsed
            .iter()
            .any(|f| f.start_line == line && f.end_line > f.start_line)
    }

    /// First document line at or after `line` that is not hidden by a collapsed fold.
    pub fn skip_to_visible(&self, mut line: usize) -> usize {
        while self.is_line_hidden(line) {
            if let Some(f) = self.collapsed.iter().find(|f| f.hides_line(line)) {
                line = f.end_line.saturating_add(1);
            } else {
                break;
            }
        }
        line
    }

    /// Move `line` by `count` visible document lines (skipping lines hidden by collapsed folds).
    pub fn move_visible_line(
        &self,
        line: usize,
        dir: crate::movement::Direction,
        count: usize,
        len_lines: usize,
    ) -> usize {
        use crate::movement::Direction;

        let max_line = len_lines.saturating_sub(1);
        let mut line = self.skip_to_visible(line.min(max_line));
        for _ in 0..count {
            line = match dir {
                Direction::Forward => self.step_visible_forward(line, max_line),
                Direction::Backward => self.step_visible_backward(line),
            };
        }
        line
    }

    pub fn step_visible_forward(&self, line: usize, max_line: usize) -> usize {
        if line >= max_line {
            return max_line;
        }
        let mut next = line + 1;
        while next <= max_line && self.is_line_hidden(next) {
            if let Some(f) = self.collapsed.iter().find(|f| f.hides_line(next)) {
                next = f.end_line.saturating_add(1);
            } else {
                next += 1;
            }
        }
        next.min(max_line)
    }

    fn step_visible_backward(&self, line: usize) -> usize {
        if line == 0 {
            return 0;
        }
        let mut prev = line - 1;
        while self.is_line_hidden(prev) {
            if let Some(f) = self.collapsed.iter().find(|f| f.hides_line(prev)) {
                if f.start_line == 0 {
                    return 0;
                }
                prev = f.start_line;
            } else {
                prev = prev.saturating_sub(1);
            }
            if prev == 0 {
                return self.skip_to_visible(0);
            }
        }
        prev
    }

    /// Innermost collapsed fold whose header is `line`, if any.
    pub fn collapsed_header_at(&self, line: usize) -> Option<&FoldRange> {
        self.collapsed
            .iter()
            .filter(|f| f.start_line == line)
            .max_by_key(|f| f.end_line)
    }

    /// Innermost available or collapsed fold containing `line`.
    pub fn innermost_containing<'a>(
        ranges: impl IntoIterator<Item = &'a FoldRange>,
        line: usize,
    ) -> Option<&'a FoldRange> {
        ranges
            .into_iter()
            .filter(|f| f.contains_line(line))
            .min_by_key(|f| f.end_line - f.start_line)
    }

    pub fn is_collapsed(&self, range: &FoldRange) -> bool {
        self.collapsed.iter().any(|f| f == range)
    }

    pub fn collapse(&mut self, range: FoldRange) {
        if !self.collapsed.iter().any(|f| f == &range) {
            self.collapsed.push(range);
            self.normalize();
        }
    }

    /// Clamp fold ranges to valid lines for the current document length.
    pub fn clamp_range(range: FoldRange, len_lines: usize) -> Option<FoldRange> {
        if len_lines == 0 {
            return None;
        }
        let max_line = len_lines - 1;
        let start_line = range.start_line.min(max_line);
        let end_line = range.end_line.min(max_line);
        FoldRange::new(start_line, end_line)
    }

    pub fn expand(&mut self, range: &FoldRange) {
        self.collapsed.retain(|f| f != range);
    }

    pub fn toggle(&mut self, range: FoldRange) {
        if self.is_collapsed(&range) {
            self.expand(&range);
        } else {
            self.collapse(range);
        }
    }

    /// Remove collapsed folds that intersect edited line range.
    pub fn invalidate_lines(&mut self, start_line: usize, end_line: usize) {
        self.collapsed
            .retain(|f| !f.intersects_lines(start_line, end_line));
    }

    pub fn collapse_all(&mut self, ranges: impl IntoIterator<Item = FoldRange>) {
        self.collapsed = ranges.into_iter().collect();
        self.normalize();
    }

    fn normalize(&mut self) {
        self.collapsed
            .sort_by(|a, b| (a.start_line, a.end_line).cmp(&(b.start_line, b.end_line)));
        self.collapsed.dedup();
    }
}

/// Byte range covering `visible_line_count` visible document lines starting at `anchor`.
///
/// Without folds this matches `height` consecutive document lines from the anchor line.
/// With folds, hidden lines are skipped so the range covers all bytes that may be rendered
/// in the viewport (required for syntax highlighting).
pub fn visible_line_byte_range(
    text: RopeSlice,
    anchor: usize,
    visible_line_count: usize,
    folds: Option<&FoldState>,
) -> std::ops::Range<usize> {
    if visible_line_count == 0 {
        let start = anchor.min(text.len_bytes());
        return start..start;
    }

    let max_line = text.len_lines().saturating_sub(1);
    let anchor = anchor.min(text.len_chars());
    let mut line = match folds.filter(|f| !f.is_empty()) {
        Some(folds) => folds.skip_to_visible(text.char_to_line(anchor)),
        None => text.char_to_line(anchor),
    };
    line = line.min(max_line);
    let start = text.line_to_byte(line);

    if visible_line_count <= 1 {
        let end = end_byte_of_line(text, line, max_line);
        return start..end;
    }

    let mut count = 1;
    while count < visible_line_count {
        if line >= max_line {
            break;
        }
        line = match folds.filter(|f| !f.is_empty()) {
            Some(folds) => folds.step_visible_forward(line, max_line),
            None => (line + 1).min(max_line),
        };
        count += 1;
    }

    start..end_byte_of_line(text, line, max_line)
}

fn end_byte_of_line(text: RopeSlice, line: usize, max_line: usize) -> usize {
    if line >= max_line {
        text.len_bytes()
    } else {
        text.line_to_byte(line + 1)
    }
}

/// Discover all fold ranges in the document using tree-sitter `folds.scm`.
pub fn discover_folds(
    syntax: &Syntax,
    loader: &crate::syntax::Loader,
    text: RopeSlice,
) -> Vec<FoldRange> {
    let mut ranges = Vec::new();
    let mut query_iter = syntax.folds(text, loader, ..);

    while let Some(event) = query_iter.next() {
        let QueryIterEvent::Match(mat) = event else {
            continue;
        };
        let fold_query = loader
            .fold_query(query_iter.current_language())
            .expect("must have a fold query to emit matches");
        if fold_query.fold_capture != Some(mat.capture) {
            continue;
        }
        if let Some(range) = FoldRange::from_byte_range(text, mat.node.byte_range()) {
            ranges.push(range);
        }
    }

    normalize_fold_ranges(ranges)
}

/// Discover fold at cursor using tree-sitter on a single node (fallback).
pub fn discover_fold_at_cursor(
    syntax: &Syntax,
    _loader: &crate::syntax::Loader,
    text: RopeSlice,
    char_idx: usize,
) -> Option<FoldRange> {
    let byte = text.char_to_byte(char_idx) as u32;
    let node = syntax.named_descendant_for_byte_range(byte, byte)?;
    FoldRange::from_byte_range(text, node.byte_range())
}

/// Returns the inclusive line range affected by a changeset (in the document before the edit).
pub fn changed_line_range(
    old_text: RopeSlice,
    changes: &crate::ChangeSet,
) -> Option<(usize, usize)> {
    use crate::Operation::*;

    let mut old_pos = 0;
    let mut start_line = usize::MAX;
    let mut end_line = 0;

    for op in changes.changes() {
        let len = op.len_chars();
        match op {
            Retain(_) => old_pos += len,
            Delete(_) => {
                let line_start = old_text.char_to_line(old_pos);
                let line_end = old_text.char_to_line((old_pos + len).min(old_text.len_chars()));
                start_line = start_line.min(line_start);
                end_line = end_line.max(line_end);
                old_pos += len;
            }
            Insert(_) => {
                let line = old_text.char_to_line(old_pos);
                start_line = start_line.min(line);
                end_line = end_line.max(line);
            }
        }
    }

    if start_line == usize::MAX {
        None
    } else {
        Some((start_line, end_line))
    }
}

/// Map a character index inside hidden folded lines to the end of the visible header line.
pub fn char_idx_for_display(folds: Option<&FoldState>, text: RopeSlice, char_idx: usize) -> usize {
    let Some(folds) = folds else {
        return char_idx;
    };
    let line = text.char_to_line(char_idx);
    if !folds.is_line_hidden(line) {
        return char_idx;
    }
    folds
        .collapsed()
        .iter()
        .find(|f| f.hides_line(line))
        .map(|f| {
            let header_line = f.start_line.min(text.len_lines().saturating_sub(1));
            if header_line + 1 < text.len_lines() {
                text.line_to_char(header_line + 1).saturating_sub(1)
            } else {
                text.len_chars().saturating_sub(1)
            }
        })
        .unwrap_or(char_idx)
}

fn normalize_fold_ranges(mut ranges: Vec<FoldRange>) -> Vec<FoldRange> {
    ranges.sort_by(|a, b| (a.start_line, a.end_line).cmp(&(b.start_line, b.end_line)));
    ranges.dedup();
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::Rope;

    #[test]
    fn fold_state_hide_and_toggle() {
        let range = FoldRange::new(1, 5).unwrap();
        let mut state = FoldState::new();
        assert!(!state.is_line_hidden(2));
        state.collapse(range.clone());
        assert!(state.is_line_hidden(2));
        assert!(!state.is_line_hidden(1));
        assert!(state.is_collapsed_header(1));
        state.toggle(range);
        assert!(!state.is_line_hidden(2));
    }

    #[test]
    fn fold_state_invalidate() {
        let mut state = FoldState::new();
        state.collapse(FoldRange::new(0, 10).unwrap());
        state.collapse(FoldRange::new(20, 30).unwrap());
        state.invalidate_lines(5, 15);
        assert_eq!(state.collapsed().len(), 1);
        assert_eq!(state.collapsed()[0].start_line, 20);
    }

    #[test]
    fn visible_line_byte_range_skips_hidden_lines() {
        let text = Rope::from((0..10).map(|i| format!("line{i}\n")).collect::<String>());
        let slice = text.slice(..);
        let mut folds = FoldState::new();
        folds.collapse(FoldRange::new(1, 4).unwrap());

        let range = visible_line_byte_range(slice, slice.line_to_char(1), 2, Some(&folds));
        assert_eq!(slice.byte_to_line(range.start), 1);
        assert_eq!(slice.byte_to_line(range.end.saturating_sub(1)), 5);
    }

    #[test]
    fn move_visible_line_skips_hidden() {
        use crate::movement::Direction;

        let mut folds = FoldState::new();
        folds.collapse(FoldRange::new(1, 3).unwrap());
        assert_eq!(folds.move_visible_line(1, Direction::Forward, 1, 5), 4);
        assert_eq!(folds.move_visible_line(4, Direction::Backward, 1, 5), 1);
    }

    #[test]
    fn folded_lines_skip_visual_rows() {
        use crate::{
            doc_formatter::TextFormat, position::visual_offset_from_block,
            text_annotations::TextAnnotations, Rope,
        };

        let text = Rope::from("line0\nline1\nline2\nline3\n");
        let slice = text.slice(..);
        let mut folds = FoldState::new();
        folds.collapse(FoldRange::new(1, 2).unwrap());

        let annotations = TextAnnotations::default().with_folds(Some(&folds));
        let text_fmt = TextFormat::default();
        let line3_start = slice.line_to_char(3);
        let (pos, _) = visual_offset_from_block(slice, 0, line3_start, &text_fmt, &annotations);
        assert_eq!(pos.row, 2);
    }

    #[test]
    fn innermost_containing() {
        let outer = FoldRange::new(0, 10).unwrap();
        let inner = FoldRange::new(2, 5).unwrap();
        let found = FoldState::innermost_containing([&outer, &inner], 3).unwrap();
        assert_eq!(found.start_line, 2);
    }
}
