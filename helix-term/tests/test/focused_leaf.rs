use helix_view::{tree::LeafKind, FocusTarget};

use super::helpers::AppBuilder;

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
