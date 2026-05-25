//! Integrated terminal runtime integration.

use std::collections::HashMap;

use helix_pty::{
    SessionHandle, TerminalCommand, TerminalEvent, TerminalRuntime, TerminalRuntimeHandle,
    TerminalScroll,
};
use helix_view::{Editor};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::job;

#[derive(Default)]
pub struct TerminalController {
    runtime: Option<TerminalRuntimeHandle>,
    events_rx: Option<UnboundedReceiver<TerminalEvent>>,
    sessions: HashMap<String, SessionHandle>,
}


impl TerminalController {
    pub fn is_running(&self) -> bool {
        self.runtime.is_some()
    }

    pub fn start(&mut self, editor: &Editor) -> anyhow::Result<()> {
        let config = crate::terminal::runtime_config(editor);
        let (handle, events_rx) = TerminalRuntime::spawn(config);
        self.runtime = Some(handle);
        self.events_rx = Some(events_rx);
        Ok(())
    }

    pub fn send(&self, cmd: TerminalCommand) {
        if let Some(handle) = &self.runtime {
            handle.send(cmd);
        }
    }

    pub fn session_handle(&self, id: &str) -> Option<&SessionHandle> {
        self.sessions.get(id)
    }

    pub fn remove_session(&mut self, id: &str) {
        self.sessions.remove(id);
    }

    pub fn register_live_session(&mut self, id: impl Into<String>, handle: SessionHandle) {
        self.sessions.insert(id.into(), handle);
    }

    pub fn shutdown(&mut self) {
        for id in self.sessions.keys().cloned().collect::<Vec<_>>() {
            self.send(TerminalCommand::Kill { id: id.into() });
        }
        self.sessions.clear();
        self.runtime = None;
        self.events_rx = None;
    }

    pub fn spawn_event_listener(&mut self, jobs: &mut crate::job::Jobs) {
        let Some(mut rx) = self.events_rx.take() else {
            return;
        };

        jobs.spawn(async move {
            while let Some(event) = rx.recv().await {
                job::dispatch(move |editor, _compositor| {
                    apply_event(editor, &event);
                })
                .await;
            }
            Ok(())
        });
    }
}

fn apply_event(editor: &mut Editor, event: &TerminalEvent) {
    match event {
        TerminalEvent::Spawned { id, handle } => {
            with_controller_store(editor, &id.0, handle.clone());
        }
        TerminalEvent::Updated { id } => {
            let is_agent_shell = editor.agent.shell_block_index.contains_key(&id.0)
                || editor.agent.tool_shell_index.values().any(|linked| linked == &id.0);
            let scroll_pinned = if is_agent_shell {
                true
            } else {
                editor
                    .terminal
                    .session_mut(&id.0)
                    .map(|session| session.scroll_pinned)
                    .unwrap_or(true)
            };

            if scroll_pinned {
                if let Some(handle) = session_handle_for(&id.0) {
                    handle.scroll(TerminalScroll::Bottom);
                }
                if !is_agent_shell {
                    if let Some(session) = editor.terminal.session_mut(&id.0) {
                        session.scroll_offset = 0;
                    }
                }
            } else if let Some(handle) = session_handle_for(&id.0) {
                if let Some(session) = editor.terminal.session_mut(&id.0) {
                    session.scroll_offset = handle.display_offset();
                }
            }

            if is_agent_shell {
                crate::agent::on_terminal_updated(editor, &id.0);
            }

            helix_event::request_redraw();
        }
        TerminalEvent::TitleChanged { id, title } => {
            if let Some(session) = editor.terminal.session_mut(&id.0) {
                session.title = Some(title.clone());
            }
            helix_event::request_redraw();
        }
        TerminalEvent::Exited { id, code, signal } => {
            let is_agent = editor.agent.shell_block_index.contains_key(&id.0)
                || editor
                    .agent
                    .tool_shell_index
                    .values()
                    .any(|linked| linked == &id.0);

            if is_agent {
                crate::terminal::shutdown_pty_session(&id.0);
                crate::agent::on_terminal_exited(editor, &id.0, *code, *signal);
            } else if editor.terminal.sessions.contains_key(&id.0) {
                crate::commands::terminal::close_terminal_session(editor, &id.0);
            } else {
                crate::terminal::shutdown_pty_session(&id.0);
                crate::terminal::with_controller(|controller| {
                    controller.remove_session(&id.0);
                });
            }

            helix_event::request_redraw();
        }
        TerminalEvent::Bell { id: _ } => {
            if editor.integrated_terminal_settings().bell {
                editor.set_status("terminal bell");
            }
            helix_event::request_redraw();
        }
        TerminalEvent::Error { text } => {
            editor.set_error(text.clone());
        }
    }
}

fn with_controller_store(_editor: &mut Editor, id: &str, handle: SessionHandle) {
    crate::terminal::with_controller(|controller| {
        controller.register_live_session(id, handle);
    });
}

fn session_handle_for(id: &str) -> Option<SessionHandle> {
    crate::terminal::session_handle(id)
}
