//! Global access to the ACP agent controller from commands and UI.

use std::sync::{Arc, Mutex};

use helix_event::runtime_local;
use once_cell::sync::OnceCell;

use crate::handlers::agent::AgentController;

runtime_local! {
    static AGENT_CONTROLLER: OnceCell<Arc<Mutex<AgentController>>> = OnceCell::new();
}

pub fn init(controller: Arc<Mutex<AgentController>>) {
    let _ = AGENT_CONTROLLER.set(controller);
}

pub fn with_controller<F, R>(f: F) -> R
where
    F: FnOnce(&mut AgentController) -> R,
{
    f(&mut AGENT_CONTROLLER.wait().lock().unwrap())
}

pub fn on_terminal_updated(editor: &mut helix_view::Editor, terminal_id: &str) {
    with_controller(|controller| controller.on_terminal_updated(editor, terminal_id));
}

pub fn on_terminal_exited(
    editor: &mut helix_view::Editor,
    terminal_id: &str,
    code: Option<i32>,
    signal: Option<i32>,
) {
    with_controller(|controller| {
        controller.on_terminal_exited(editor, terminal_id, code, signal);
    });
}

/// Start the ACP runtime if needed, without creating a new agent session.
pub fn ensure_runtime_started(
    editor: &helix_view::Editor,
    jobs: &mut crate::job::Jobs,
) -> anyhow::Result<()> {
    crate::terminal::ensure_headless_runtime_started(editor, jobs)?;
    with_controller(|controller| {
        if !controller.is_running() {
            controller.start(editor)?;
            controller.spawn_event_listener(jobs);
        }
        Ok(())
    })
}

pub fn ensure_runtime(
    editor: &mut helix_view::Editor,
    jobs: &mut crate::job::Jobs,
) -> anyhow::Result<()> {
    ensure_runtime_started(editor, jobs)?;
    with_controller(|controller| {
        crate::handlers::agent::ensure_session(controller, editor);
    });
    Ok(())
}
