//! Grid text extraction helpers for terminal scrollback selection and search.

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{point_to_viewport, viewport_to_point, Term};

/// A point in the terminal grid (alacritty line/column coordinates).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridPoint {
    pub line: i32,
    pub col: usize,
}

/// Character-wise or line-wise selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionKind {
    Char,
    Line,
}

/// Extract visible line text at an alacritty grid line.
pub fn line_text<L: EventListener>(term: &Term<L>, line: Line) -> String {
    let cols = term.columns();
    let mut text = String::with_capacity(cols);
    for col in 0..cols {
        let cell = &term.grid()[line][Column(col)];
        if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            continue;
        }
        text.push(cell.c);
    }
    text.trim_end().to_string()
}

/// All lines from scrollback top through the active screen (inclusive).
pub fn all_lines<L: EventListener>(term: &Term<L>) -> Vec<(i32, String)> {
    let top = term.topmost_line().0;
    let bottom = term.bottommost_line().0;
    (top..=bottom)
        .map(|line| {
            let grid_line = Line(line);
            (line, line_text(term, grid_line))
        })
        .collect()
}

/// Map a viewport-relative row/column to a grid point.
pub fn viewport_point_to_grid<L: EventListener>(
    term: &Term<L>,
    viewport_row: usize,
    col: usize,
) -> GridPoint {
    let display_offset = term.grid().display_offset();
    let point = viewport_to_point(display_offset, Point::new(viewport_row, Column(col)));
    GridPoint {
        line: point.line.0,
        col: point.column.0,
    }
}

/// Map a grid point to viewport coordinates, if visible.
pub fn grid_point_to_viewport<L: EventListener>(
    term: &Term<L>,
    point: GridPoint,
) -> Option<(usize, usize)> {
    let display_offset = term.grid().display_offset();
    let grid_point = Point::new(Line(point.line), Column(point.col));
    point_to_viewport(display_offset, grid_point).map(|p| (p.line, p.column.0))
}

fn normalized_points(anchor: GridPoint, head: GridPoint) -> (GridPoint, GridPoint) {
    if anchor.line < head.line || (anchor.line == head.line && anchor.col <= head.col) {
        (anchor, head)
    } else {
        (head, anchor)
    }
}

/// Extract selected text between two grid points.
pub fn range_text<L: EventListener>(
    term: &Term<L>,
    anchor: GridPoint,
    head: GridPoint,
    kind: SelectionKind,
) -> String {
    let cols = term.columns();
    let (start, end) = normalized_points(anchor, head);

    if kind == SelectionKind::Line {
        let mut out = String::new();
        for line in start.line..=end.line {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&line_text(term, Line(line)));
        }
        return out;
    }

    if start.line == end.line {
        let text = line_text(term, Line(start.line));
        let end_col = end.col.min(cols.saturating_sub(1));
        let start_col = start.col.min(end_col);
        return slice_line_columns(&text, start_col, end_col + 1);
    }

    let mut out = String::new();
    for line in start.line..=end.line {
        if line > start.line {
            out.push('\n');
        }
        let text = line_text(term, Line(line));
        let slice = if line == start.line {
            slice_line_columns(&text, start.col, cols)
        } else if line == end.line {
            slice_line_columns(&text, 0, end.col.min(cols.saturating_sub(1)) + 1)
        } else {
            text
        };
        out.push_str(&slice);
    }
    out
}

fn slice_line_columns(text: &str, start_col: usize, end_col: usize) -> String {
    if start_col >= end_col {
        return String::new();
    }
    text.chars()
        .enumerate()
        .filter_map(|(idx, c)| (idx >= start_col && idx < end_col).then_some(c))
        .collect()
}

impl crate::live::SessionHandle {
    pub fn line_text_at(&self, line: i32) -> String {
        self.with_term(|term| line_text(term, Line(line)))
    }

    pub fn all_lines(&self) -> Vec<(i32, String)> {
        self.with_term(all_lines)
    }

    pub fn viewport_point_to_grid(&self, viewport_row: usize, col: usize) -> GridPoint {
        self.with_term(|term| viewport_point_to_grid(term, viewport_row, col))
    }

    pub fn grid_point_to_viewport(&self, point: GridPoint) -> Option<(usize, usize)> {
        self.with_term(|term| grid_point_to_viewport(term, point))
    }

    pub fn range_text(&self, anchor: GridPoint, head: GridPoint, kind: SelectionKind) -> String {
        self.with_term(|term| range_text(term, anchor, head, kind))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::event::{Event, EventListener};
    use alacritty_terminal::grid::Dimensions;
    use alacritty_terminal::term::Config;
    use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};

    struct NullListener;
    impl EventListener for NullListener {
        fn send_event(&self, _event: Event) {}
    }

    struct TermSize {
        columns: usize,
        screen_lines: usize,
    }

    impl Dimensions for TermSize {
        fn total_lines(&self) -> usize {
            self.screen_lines
        }

        fn screen_lines(&self) -> usize {
            self.screen_lines
        }

        fn columns(&self) -> usize {
            self.columns
        }
    }

    fn test_term(rows: u16, cols: u16) -> Term<NullListener> {
        let size = TermSize {
            columns: cols as usize,
            screen_lines: rows as usize,
        };
        Term::new(Config::default(), &size, NullListener)
    }

    #[test]
    fn line_text_trims_trailing_spaces() {
        let mut term = test_term(5, 10);
        let mut parser = Processor::<StdSyncHandler>::new();
        parser.advance(&mut term, b"hello     ");
        assert_eq!(line_text(&term, Line(0)), "hello");
    }

    #[test]
    fn range_text_selects_char_range() {
        let mut term = test_term(5, 10);
        let mut parser = Processor::<StdSyncHandler>::new();
        parser.advance(&mut term, b"abcdef");
        let text = range_text(
            &term,
            GridPoint { line: 0, col: 1 },
            GridPoint { line: 0, col: 3 },
            SelectionKind::Char,
        );
        assert_eq!(text, "bcd");
    }

    #[test]
    fn range_text_line_mode_includes_full_lines() {
        let mut term = test_term(5, 10);
        let mut parser = Processor::<StdSyncHandler>::new();
        parser.advance(&mut term, b"line0\r\nline1\r\nline2");
        let text = range_text(
            &term,
            GridPoint { line: 0, col: 2 },
            GridPoint { line: 1, col: 1 },
            SelectionKind::Line,
        );
        assert_eq!(text, "line0\nline1");
    }
}
