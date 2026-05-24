use std::collections::HashMap;
use std::path::PathBuf;

use alacritty_terminal::tty::Shell;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use crate::events::{TerminalCommand, TerminalEvent};
use crate::live::{spawn_live_session, LiveSession, TerminalSpawnConfig};
use crate::session::TerminalId;

const DEFAULT_SCROLLBACK: usize = 10_000;

/// Handle for sending commands to the terminal runtime.
#[derive(Clone)]
pub struct TerminalRuntimeHandle {
    cmd_tx: UnboundedSender<TerminalCommand>,
}

impl TerminalRuntimeHandle {
    pub fn send(&self, cmd: TerminalCommand) {
        let _ = self.cmd_tx.send(cmd);
    }
}

/// Configuration for the terminal runtime.
#[derive(Debug, Clone)]
pub struct TerminalConfig {
    pub scrollback_lines: usize,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            scrollback_lines: DEFAULT_SCROLLBACK,
        }
    }
}

/// Background terminal runtime.
pub struct TerminalRuntime;

impl TerminalRuntime {
    pub fn spawn(
        config: TerminalConfig,
    ) -> (TerminalRuntimeHandle, UnboundedReceiver<TerminalEvent>) {
        let (events_tx, events_rx) = unbounded_channel();
        let (cmd_tx, mut cmd_rx) = unbounded_channel();

        tokio::task::spawn(async move {
            let mut sessions: HashMap<TerminalId, LiveSession> = HashMap::new();
            let spawn_config = TerminalSpawnConfig {
                scrollback_lines: config.scrollback_lines,
                shell: None,
                env: HashMap::new(),
            };

            while let Some(command) = cmd_rx.recv().await {
                match command {
                    TerminalCommand::Spawn { id, cwd, rows, cols } => {
                        match spawn_live_session(
                            id.clone(),
                            cwd,
                            rows,
                            cols,
                            &spawn_config,
                            events_tx.clone(),
                        ) {
                            Ok(session) => {
                                let handle = session.handle.clone();
                                sessions.insert(id.clone(), session);
                                let _ = events_tx.send(TerminalEvent::Spawned { id, handle });
                            }
                            Err(err) => {
                                let _ = events_tx.send(TerminalEvent::Error {
                                    text: format!("failed to spawn terminal: {err:#}"),
                                });
                            }
                        }
                    }
                    TerminalCommand::SpawnProgram {
                        id,
                        command,
                        args,
                        env,
                        cwd,
                        rows,
                        cols,
                    } => {
                        let mut config = spawn_config.clone();
                        config.shell = Some(Shell::new(command, args));
                        config.env = env.into_iter().collect();
                        match spawn_live_session(
                            id.clone(),
                            cwd,
                            rows,
                            cols,
                            &config,
                            events_tx.clone(),
                        ) {
                            Ok(session) => {
                                let handle = session.handle.clone();
                                sessions.insert(id.clone(), session);
                                let _ = events_tx.send(TerminalEvent::Spawned { id, handle });
                            }
                            Err(err) => {
                                let _ = events_tx.send(TerminalEvent::Error {
                                    text: format!("failed to spawn program: {err:#}"),
                                });
                            }
                        }
                    }
                    TerminalCommand::Write { id, data } => {
                        let Some(session) = sessions.get(&id) else {
                            let _ = events_tx.send(TerminalEvent::Error {
                                text: format!("unknown terminal session: {id}"),
                            });
                            continue;
                        };
                        session.handle.write(&data);
                    }
                    TerminalCommand::Resize { id, rows, cols } => {
                        if let Some(session) = sessions.get(&id) {
                            session.handle.resize(rows, cols);
                        }
                    }
                    TerminalCommand::Kill { id } => {
                        if let Some(session) = sessions.remove(&id) {
                            session.shutdown();
                        }
                    }
                }
            }

            for (_, session) in sessions.drain() {
                session.shutdown();
            }
        });

        (TerminalRuntimeHandle { cmd_tx }, events_rx)
    }
}

#[allow(dead_code)]
fn resolve_cwd(cwd: Option<PathBuf>) -> PathBuf {
    cwd.or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}
