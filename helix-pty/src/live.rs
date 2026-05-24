use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;

#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, EventLoopSender, Msg};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::tty::{self, Shell};
use tokio::sync::mpsc::UnboundedSender;

use crate::events::TerminalEvent;
use crate::session::TerminalId;

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

/// Forwards alacritty terminal events to the Helix runtime channel.
#[derive(Clone)]
pub struct ChannelListener {
    id: TerminalId,
    tx: UnboundedSender<TerminalEvent>,
}

impl EventListener for ChannelListener {
    fn send_event(&self, event: Event) {
        match event {
            Event::Wakeup => {
                let _ = self.tx.send(TerminalEvent::Updated {
                    id: self.id.clone(),
                });
            }
            Event::Title(title) => {
                let _ = self.tx.send(TerminalEvent::TitleChanged {
                    id: self.id.clone(),
                    title,
                });
            }
            Event::ChildExit(status) => {
                #[cfg(unix)]
                let signal = status.signal();
                #[cfg(not(unix))]
                let signal = None;
                let _ = self.tx.send(TerminalEvent::Exited {
                    id: self.id.clone(),
                    code: status.code(),
                    signal,
                });
            }
            Event::Bell => {
                let _ = self.tx.send(TerminalEvent::Bell {
                    id: self.id.clone(),
                });
            }
            _ => {}
        }
    }
}

/// Handle to a live PTY session (grid + input channel).
#[derive(Clone)]
pub struct SessionHandle {
    pub id: TerminalId,
    pub term: Arc<FairMutex<Term<ChannelListener>>>,
    input: EventLoopSender,
}

impl SessionHandle {
    pub fn write(&self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        let _ = self
            .input
            .send(Msg::Input(Cow::Owned(data.to_vec())));
    }

    pub fn resize(&self, rows: u16, cols: u16) {
        let _ = self.input.send(Msg::Resize(WindowSize {
            num_lines: rows,
            num_cols: cols,
            cell_width: 1,
            cell_height: 1,
        }));
    }

    pub fn shutdown(&self) {
        let _ = self.input.send(Msg::Shutdown);
    }

    pub fn with_term<R>(&self, f: impl FnOnce(&Term<ChannelListener>) -> R) -> R {
        let guard = self.term.lock();
        f(&guard)
    }

    pub fn with_term_mut<R>(&self, f: impl FnOnce(&mut Term<ChannelListener>) -> R) -> R {
        let mut guard = self.term.lock();
        f(&mut guard)
    }

    pub fn scroll(&self, scroll: Scroll) {
        self.with_term_mut(|term| term.scroll_display(scroll));
    }

    pub fn display_offset(&self) -> usize {
        self.with_term(|term| term.grid().display_offset())
    }
}

pub struct LiveSession {
    pub handle: SessionHandle,
    shutdown: Option<Box<dyn FnOnce() + Send>>,
}

impl LiveSession {
    pub fn shutdown(mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            shutdown();
        }
    }
}

/// Configuration for spawning live terminal sessions.
#[derive(Debug, Clone, Default)]
pub struct TerminalSpawnConfig {
    pub scrollback_lines: usize,
    pub shell: Option<Shell>,
}

pub fn spawn_live_session(
    id: TerminalId,
    cwd: Option<PathBuf>,
    rows: u16,
    cols: u16,
    config: &TerminalSpawnConfig,
    events_tx: UnboundedSender<TerminalEvent>,
) -> anyhow::Result<LiveSession> {
    tty::setup_env();

    let rows = rows.max(1);
    let cols = cols.max(2);
    let window_size = WindowSize {
        num_lines: rows,
        num_cols: cols,
        cell_width: 1,
        cell_height: 1,
    };

    let options = tty::Options {
        shell: config.shell.clone(),
        working_directory: cwd,
        drain_on_exit: true,
        ..Default::default()
    };

    let pty = tty::new(&options, window_size, 0)?;

    let term_config = Config {
        scrolling_history: config.scrollback_lines,
        ..Default::default()
    };

    let size = TermSize {
        columns: cols as usize,
        screen_lines: rows as usize,
    };

    let proxy = ChannelListener {
        id: id.clone(),
        tx: events_tx,
    };
    let term = Term::new(term_config, &size, proxy.clone());
    let term = Arc::new(FairMutex::new(term));

    let event_loop = EventLoop::new(term.clone(), proxy, pty, true, false)?;
    let input = event_loop.channel();
    let join = event_loop.spawn();

    let handle = SessionHandle {
        id: id.clone(),
        term,
        input: input.clone(),
    };

    let shutdown = move || {
        let _ = input.send(Msg::Shutdown);
        let _ = join.join();
    };

    Ok(LiveSession {
        handle,
        shutdown: Some(Box::new(shutdown)),
    })
}
