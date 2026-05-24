use std::time::Duration;

use helix_pty::{TerminalCommand, TerminalConfig, TerminalEvent, TerminalId, TerminalRuntime};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::time::timeout;

async fn collect_events(
    rx: &mut UnboundedReceiver<TerminalEvent>,
    duration: Duration,
) -> Vec<TerminalEvent> {
    let mut events = Vec::new();
    while let Ok(Some(event)) = timeout(duration, rx.recv()).await {
        events.push(event);
    }
    events
}

#[tokio::test(flavor = "multi_thread")]
async fn live_session_runs_shell_and_exits() {
    let (handle, mut rx) = TerminalRuntime::spawn(TerminalConfig::default());
    let id = TerminalId::from("test-live".to_string());

    let cwd = std::env::current_dir().ok();
    handle.send(TerminalCommand::Spawn {
        id: id.clone(),
        cwd,
        rows: 24,
        cols: 80,
    });

    let mut events = collect_events(&mut rx, Duration::from_secs(3)).await;
    assert!(
        events.iter().any(|event| matches!(event, TerminalEvent::Spawned { .. })),
        "expected Spawned event, got {events:?}"
    );

    handle.send(TerminalCommand::Write {
        id: id.clone(),
        data: b"echo helix-pty-live\n".to_vec(),
    });

    events = collect_events(&mut rx, Duration::from_secs(2)).await;
    assert!(
        events.iter().any(|event| matches!(event, TerminalEvent::Updated { .. })),
        "expected Updated after shell output, got {events:?}"
    );

    handle.send(TerminalCommand::Kill { id: id.clone() });
    let _ = collect_events(&mut rx, Duration::from_secs(2)).await;
}
