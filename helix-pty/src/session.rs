use std::fmt;
use std::sync::Arc;

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::Processor;
use parking_lot::Mutex;

/// Opaque terminal session identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TerminalId(pub String);

impl fmt::Display for TerminalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for TerminalId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

struct TerminalSize {
    columns: usize,
    screen_lines: usize,
}

impl Dimensions for TerminalSize {
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

#[derive(Clone)]
struct SessionListener {
    title: Arc<Mutex<Option<String>>>,
}

impl EventListener for SessionListener {
    fn send_event(&self, event: Event) {
        match event {
            Event::Title(title) => *self.title.lock() = Some(title),
            Event::ResetTitle => *self.title.lock() = None,
            _ => {}
        }
    }
}

/// In-memory terminal emulator session backed by alacritty's grid.
pub struct TerminalSession {
    term: Term<SessionListener>,
    parser: Processor,
    title: Arc<Mutex<Option<String>>>,
    scrollback_lines: usize,
}

impl TerminalSession {
    pub fn new(rows: u16, cols: u16, scrollback_lines: usize) -> Self {
        let mut config = Config::default();
        config.scrolling_history = scrollback_lines;
        let size = TerminalSize {
            columns: cols.max(2) as usize,
            screen_lines: rows.max(1) as usize,
        };
        let title = Arc::new(Mutex::new(None));
        let listener = SessionListener {
            title: title.clone(),
        };
        let term = Term::new(config, &size, listener);

        Self {
            term,
            parser: Processor::new(),
            title,
            scrollback_lines,
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.term, bytes);
    }

    pub fn resize(&mut self, rows: u16, cols: u16) {
        let size = TerminalSize {
            columns: cols.max(2) as usize,
            screen_lines: rows.max(1) as usize,
        };
        self.term.resize(size);
    }

    pub fn rows(&self) -> u16 {
        self.term.screen_lines() as u16
    }

    pub fn cols(&self) -> u16 {
        self.term.columns() as u16
    }

    pub fn scrollback_lines(&self) -> usize {
        self.scrollback_lines
    }

    /// Visible line text at `row` (0 = top of viewport).
    pub fn line(&self, row: usize) -> String {
        if row >= self.term.screen_lines() {
            return String::new();
        }

        let line = Line(row as i32);
        let cols = self.term.columns();
        let mut text = String::with_capacity(cols);
        for col in 0..cols {
            text.push(self.term.grid()[line][Column(col)].c);
        }
        text.trim_end().to_string()
    }

    pub fn scrollback_len(&self) -> usize {
        self.term.history_size()
    }

    pub fn title(&self) -> Option<String> {
        self.title.lock().clone()
    }
}
