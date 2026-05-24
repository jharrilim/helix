use std::path::PathBuf;

use crate::live::SessionHandle;
use crate::session::TerminalId;

/// Commands sent from the UI layer to the terminal runtime.
#[derive(Debug, Clone)]
pub enum TerminalCommand {
    Spawn {
        id: TerminalId,
        cwd: Option<PathBuf>,
        rows: u16,
        cols: u16,
    },
    /// Spawn a specific program (used by ACP `terminal/create`).
    SpawnProgram {
        id: TerminalId,
        command: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
        cwd: Option<PathBuf>,
        rows: u16,
        cols: u16,
    },
    Write {
        id: TerminalId,
        data: Vec<u8>,
    },
    Resize {
        id: TerminalId,
        rows: u16,
        cols: u16,
    },
    Kill {
        id: TerminalId,
    },
}

/// Events emitted by the terminal runtime to the UI layer.
#[derive(Clone)]
pub enum TerminalEvent {
    Spawned {
        id: TerminalId,
        handle: SessionHandle,
    },
    Updated {
        id: TerminalId,
    },
    TitleChanged {
        id: TerminalId,
        title: String,
    },
    Exited {
        id: TerminalId,
        code: Option<i32>,
        signal: Option<i32>,
    },
    Bell {
        id: TerminalId,
    },
    Error {
        text: String,
    },
}

impl std::fmt::Debug for TerminalEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spawned { id, .. } => f.debug_struct("Spawned").field("id", id).finish(),
            Self::Updated { id } => f.debug_struct("Updated").field("id", id).finish(),
            Self::TitleChanged { id, title } => f
                .debug_struct("TitleChanged")
                .field("id", id)
                .field("title", title)
                .finish(),
            Self::Exited { id, code, signal } => f
                .debug_struct("Exited")
                .field("id", id)
                .field("code", code)
                .field("signal", signal)
                .finish(),
            Self::Bell { id } => f.debug_struct("Bell").field("id", id).finish(),
            Self::Error { text } => f.debug_struct("Error").field("text", text).finish(),
        }
    }
}
