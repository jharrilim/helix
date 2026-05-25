use helix_core::diagnostic::Severity;
use helix_term::application::Application;
use helix_view::{tree::LeafKind, FocusTarget};

use helix_view::input::parse_macro;
use tokio_stream::wrappers::UnboundedReceiverStream;

#[cfg(not(windows))]
use termina::event::{Event, KeyEvent};

use super::helpers::{run_event_loop_until_idle, AppBuilder};

const DOCUMENT_VIEW_REQUIRED: &str = "command requires a document view";

fn expect_document_view_error(app: &mut Application, command: &str) {
    app.validate_typed_command(command, "")
        .expect("command dispatch should not fail");
    let (status, severity) = app.editor.get_status().unwrap();
    assert_eq!(*severity, Severity::Error, "command :{command}");
    assert_eq!(status.as_ref(), DOCUMENT_VIEW_REQUIRED);
}

#[tokio::test(flavor = "multi_thread")]
async fn document_view_focus_is_detected() -> anyhow::Result<()> {
    let app = AppBuilder::new().build()?;
    assert!(app.editor.is_document_view_focused());
    assert_eq!(app.editor.focused_leaf_kind(), Some(LeafKind::View));
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn git_panel_focus_is_not_document_view() -> anyhow::Result<()> {
    let mut app = AppBuilder::new().build()?;
    app.editor.open_git_panel();
    assert_eq!(app.editor.focused_leaf_kind(), Some(LeafKind::GitPanel));
    assert!(!app.editor.is_document_view_focused());
    assert!(app.editor.tree.try_focused_view().is_none());
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_panel_focus_is_not_document_view() -> anyhow::Result<()> {
    let mut app = AppBuilder::new().build()?;
    app.editor.open_agent_panel();
    assert_eq!(app.editor.focused_leaf_kind(), Some(LeafKind::AgentPanel));
    assert!(!app.editor.is_document_view_focused());
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn focus_target_reports_panel_kind() -> anyhow::Result<()> {
    let mut app = AppBuilder::new().build()?;
    app.editor.open_git_panel();
    assert!(matches!(
        app.editor.focus_target(),
        Some(FocusTarget::Git(_))
    ));
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn close_git_panel_from_focus_does_not_panic() -> anyhow::Result<()> {
    let mut app = AppBuilder::new().build()?;
    app.editor.open_git_panel();
    app.editor.close_git_panel();
    assert!(app.editor.is_document_view_focused());
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn quit_all_closes_git_panel() -> anyhow::Result<()> {
    let mut app = AppBuilder::new().build()?;
    app.editor.open_git_panel();
    app.editor.close_agent_panel();
    app.editor.close_terminal_panel();
    app.editor.close_git_panel();
    assert!(app.editor.git.panel_id.is_none());
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn typed_write_from_git_panel_errors() -> anyhow::Result<()> {
    let mut app = AppBuilder::new().with_file("foo.txt", None).build()?;
    app.editor.open_git_panel();
    expect_document_view_error(&mut app, "write");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn typed_format_from_git_panel_errors() -> anyhow::Result<()> {
    let mut app = AppBuilder::new().with_file("foo.txt", None).build()?;
    app.editor.open_git_panel();
    expect_document_view_error(&mut app, "format");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn typed_buffer_next_from_git_panel_errors() -> anyhow::Result<()> {
    let mut app = AppBuilder::new().with_file("foo.txt", None).build()?;
    app.editor.open_git_panel();
    expect_document_view_error(&mut app, "buffer-next");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn typed_vsplit_from_git_panel_errors() -> anyhow::Result<()> {
    let mut app = AppBuilder::new().with_file("foo.txt", None).build()?;
    app.editor.open_git_panel();
    expect_document_view_error(&mut app, "vsplit");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn typed_yank_join_from_agent_panel_errors() -> anyhow::Result<()> {
    let mut app = AppBuilder::new().with_file("foo.txt", None).build()?;
    app.editor.open_agent_panel();
    expect_document_view_error(&mut app, "yank-join");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn typed_quit_from_git_panel_closes_panel() -> anyhow::Result<()> {
    let mut app = AppBuilder::new().with_file("foo.txt", None).build()?;
    app.editor.open_git_panel();
    app.validate_typed_command("quit", "")?;
    assert!(app.editor.git.panel_id.is_none());
    assert!(!app.editor.is_err());
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn git_panel_space_w_does_not_panic() -> anyhow::Result<()> {
    let mut app = AppBuilder::new().build()?;
    app.editor.open_git_panel();

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let mut rx_stream = UnboundedReceiverStream::new(rx);
    for key_event in parse_macro("<space>w")? {
        tx.send(Ok(Event::Key(KeyEvent::from(key_event))))?;
    }
    app.event_loop_until_idle(&mut rx_stream).await;

    assert_eq!(app.editor.focused_leaf_kind(), Some(LeafKind::GitPanel));
    app.editor.close_git_panel();
    run_event_loop_until_idle(&mut app).await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn typed_agent_open_from_git_panel_succeeds() -> anyhow::Result<()> {
    let mut app = AppBuilder::new().with_file("foo.txt", None).build()?;
    app.editor.open_git_panel();
    app.validate_typed_command("agent-open", "")?;
    assert!(app.editor.agent.panel_id.is_some());
    Ok(())
}
