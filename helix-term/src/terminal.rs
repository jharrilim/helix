//! Global access to the integrated terminal controller from commands and UI.

use std::sync::{Arc, Mutex};

use helix_event::runtime_local;
use helix_pty::{SessionHandle, TerminalCommand, TerminalConfig};
use once_cell::sync::OnceCell;

use crate::handlers::terminal::TerminalController;

runtime_local! {
    static TERMINAL_CONTROLLER: OnceCell<Arc<Mutex<TerminalController>>> = OnceCell::new();
}

pub fn init(controller: Arc<Mutex<TerminalController>>) {
    let _ = TERMINAL_CONTROLLER.set(controller);
}

pub fn with_controller<F, R>(f: F) -> R
where
    F: FnOnce(&mut TerminalController) -> R,
{
    f(&mut TERMINAL_CONTROLLER.wait().lock().unwrap())
}

pub fn session_handle(id: &str) -> Option<SessionHandle> {
    with_controller(|controller| controller.session_handle(id).cloned())
}

/// Start the terminal runtime for agent shell blocks (does not require integrated terminal UI).
pub fn ensure_headless_runtime_started(
    editor: &helix_view::Editor,
    jobs: &mut crate::job::Jobs,
) -> anyhow::Result<()> {
    with_controller(|controller| {
        if !controller.is_running() {
            controller.start(editor)?;
            controller.spawn_event_listener(jobs);
        }
        Ok(())
    })
}

/// Start the terminal runtime if needed.
pub fn ensure_runtime_started(
    editor: &helix_view::Editor,
    jobs: &mut crate::job::Jobs,
) -> anyhow::Result<()> {
    if !editor.integrated_terminal_settings().enable {
        anyhow::bail!("integrated terminal is disabled in config");
    }

    with_controller(|controller| {
        if !controller.is_running() {
            controller.start(editor)?;
            controller.spawn_event_listener(jobs);
        }
        Ok(())
    })
}

pub fn send(cmd: TerminalCommand) {
    with_controller(|controller| controller.send(cmd));
}

/// Tear down the live PTY for `id` after the child process has exited.
/// Keeps the session handle registered so scrollback remains readable in the UI.
pub fn shutdown_pty_session(id: &str) {
    send(TerminalCommand::Kill {
        id: id.to_string().into(),
    });
}

pub fn shutdown_all() {
    with_controller(|controller| controller.shutdown());
}

pub fn runtime_config(editor: &helix_view::Editor) -> TerminalConfig {
    let settings = editor.integrated_terminal_settings();
    TerminalConfig {
        scrollback_lines: settings.scrollback_lines,
    }
}
