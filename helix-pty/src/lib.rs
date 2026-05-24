//! PTY and terminal emulator plumbing for Helix.

mod events;
mod live;
mod runtime;
mod session;

pub use events::{TerminalCommand, TerminalEvent};
pub use alacritty_terminal::grid::Scroll as TerminalScroll;
pub use live::{ChannelListener, SessionHandle, TerminalSpawnConfig};
pub use runtime::{TerminalConfig, TerminalRuntime, TerminalRuntimeHandle};
pub use session::{TerminalId, TerminalSession};
