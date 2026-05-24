//! PTY and terminal emulator plumbing for Helix.

mod events;
mod grid_text;
mod live;
mod runtime;
mod session;

pub use events::{TerminalCommand, TerminalEvent};
pub use grid_text::{all_lines, line_text, GridPoint, SelectionKind, viewport_point_to_grid};
pub use alacritty_terminal::grid::Scroll as TerminalScroll;
pub use live::{ChannelListener, SessionHandle, TerminalSpawnConfig};
pub use runtime::{TerminalConfig, TerminalRuntime, TerminalRuntimeHandle};
pub use session::{TerminalId, TerminalSession};
