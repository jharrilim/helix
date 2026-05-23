use helix_acp::{AgentEvent, AgentMessage, AgentSessionId};
use helix_term::apply_test_agent_event;

use super::helpers::{run_event_loop_until_idle, AppBuilder};

fn session_id(id: &str) -> AgentSessionId {
    AgentSessionId(id.into())
}

fn open_agent_panel(app: &mut helix_term::application::Application) {
    app.editor.open_agent_panel();
    app.editor.focus_agent_panel();
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_transcript_renders_roles() -> anyhow::Result<()> {
    let mut app = AppBuilder::new().build()?;
    let id = session_id("session-roles");

    open_agent_panel(&mut app);
    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::SessionStarted {
            session_id: id.clone(),
        },
    );
    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::Message(AgentMessage::User {
            text: "hello from user".into(),
        }),
    );
    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::Message(AgentMessage::Assistant {
            text: "hello from assistant".into(),
        }),
    );

    app.render_frame().await;
    let buffer = app.test_buffer_string();

    assert!(buffer.contains("user:"), "expected user role label\n{buffer}");
    assert!(
        buffer.contains("hello from user"),
        "expected user message body\n{buffer}"
    );
    assert!(buffer.contains("assistant:"), "expected assistant role label\n{buffer}");
    assert!(
        buffer.contains("hello from assistant"),
        "expected assistant message body\n{buffer}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_transcript_renders_markdown_body() -> anyhow::Result<()> {
    let mut app = AppBuilder::new().build()?;
    let id = session_id("session-markdown");

    open_agent_panel(&mut app);
    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::SessionStarted {
            session_id: id.clone(),
        },
    );
    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::Message(AgentMessage::Assistant {
            text: "# Heading\n\n**bold phrase** and `inline`".into(),
        }),
    );

    app.render_frame().await;
    let buffer = app.test_buffer_string();

    assert!(
        buffer.contains("Heading"),
        "expected markdown heading text\n{buffer}"
    );
    assert!(
        buffer.contains("bold phrase"),
        "expected markdown bold text\n{buffer}"
    );
    assert!(
        buffer.contains("inline"),
        "expected markdown inline code text\n{buffer}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_panel_shows_session_title() -> anyhow::Result<()> {
    let mut app = AppBuilder::new().build()?;
    let id = session_id("session-title");

    open_agent_panel(&mut app);
    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::SessionStarted {
            session_id: id.clone(),
        },
    );
    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::SessionInfoUpdated {
            title: Some(Some("My Test Session".into())),
            updated_at: None,
        },
    );

    app.render_frame().await;
    let buffer = app.test_buffer_string();

    assert!(
        buffer.contains("Agent: My Test Session"),
        "expected session title in panel border\n{buffer}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_session_load_replays_transcript() -> anyhow::Result<()> {
    let mut app = AppBuilder::new().build()?;
    let id = session_id("session-replay");

    open_agent_panel(&mut app);
    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::SessionStarted {
            session_id: id.clone(),
        },
    );
    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::Message(AgentMessage::User {
            text: "stale transcript line".into(),
        }),
    );

    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::SessionLoadStarted {
            session_id: id.clone(),
        },
    );
    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::Message(AgentMessage::User {
            text: "replayed user line".into(),
        }),
    );
    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::Message(AgentMessage::Assistant {
            text: "replayed assistant line".into(),
        }),
    );
    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::SessionLoaded {
            session_id: id,
        },
    );

    app.render_frame().await;
    let buffer = app.test_buffer_string();

    assert!(
        !buffer.contains("stale transcript line"),
        "expected prior transcript to be cleared on load\n{buffer}"
    );
    assert!(
        buffer.contains("replayed user line"),
        "expected replayed user message\n{buffer}"
    );
    assert!(
        buffer.contains("replayed assistant line"),
        "expected replayed assistant message\n{buffer}"
    );
    assert!(
        buffer.contains("agent session loaded"),
        "expected loaded status in header\n{buffer}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_session_load_ignores_empty_replay_chunks() -> anyhow::Result<()> {
    let mut app = AppBuilder::new().build()?;
    let id = session_id("session-replay-empty-chunks");

    open_agent_panel(&mut app);
    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::SessionLoadStarted {
            session_id: id.clone(),
        },
    );
    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::Message(AgentMessage::Assistant {
            text: "visible replayed transcript".into(),
        }),
    );

    for _ in 0..200 {
        apply_test_agent_event(
            &mut app.editor,
            AgentEvent::Message(AgentMessage::User {
                text: String::new(),
            }),
        );
        apply_test_agent_event(
            &mut app.editor,
            AgentEvent::Message(AgentMessage::Assistant {
                text: String::new(),
            }),
        );
    }

    apply_test_agent_event(&mut app.editor, AgentEvent::SessionLoaded { session_id: id });

    app.render_frame().await;
    let buffer = app.test_buffer_string();

    assert!(
        buffer.contains("visible replayed transcript"),
        "expected empty replay chunks not to scroll transcript out of view\n{buffer}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_transcript_survives_idle_loop() -> anyhow::Result<()> {
    let mut app = AppBuilder::new().build()?;
    let id = session_id("session-idle");

    open_agent_panel(&mut app);
    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::SessionStarted {
            session_id: id,
        },
    );
    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::Message(AgentMessage::User {
            text: "idle loop check".into(),
        }),
    );

    run_event_loop_until_idle(&mut app).await;
    app.render_frame().await;
    let buffer = app.test_buffer_string();

    assert!(
        buffer.contains("idle loop check"),
        "expected transcript after idle processing\n{buffer}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_header_shows_mode() -> anyhow::Result<()> {
    let mut app = AppBuilder::new().build()?;

    open_agent_panel(&mut app);
    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::ModeUpdated {
            current_mode: "agent".into(),
            available_modes: vec![helix_acp::AgentModeInfo {
                id: "agent".into(),
                name: "Agent".into(),
                description: None,
            }],
        },
    );

    app.render_frame().await;
    let buffer = app.test_buffer_string();

    assert!(
        buffer.contains("mode:agent"),
        "expected mode in agent header\n{buffer}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_transcript_renders_cursor_plan_notification() -> anyhow::Result<()> {
    let mut app = AppBuilder::new().build()?;

    open_agent_panel(&mut app);
    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::Message(AgentMessage::Plan {
            entries: vec!["[pending] Inspect auth flow".into()],
        }),
    );

    app.render_frame().await;
    let buffer = app.test_buffer_string();

    assert!(
        buffer.contains("Inspect auth flow"),
        "expected plan entry in transcript\n{buffer}"
    );

    Ok(())
}
