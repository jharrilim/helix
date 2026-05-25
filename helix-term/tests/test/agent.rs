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

#[tokio::test(flavor = "multi_thread")]
async fn agent_transcript_scroll_clamps_at_top() -> anyhow::Result<()> {
    let mut app = AppBuilder::new().build()?;

    open_agent_panel(&mut app);
    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::Message(AgentMessage::User {
            text: "first transcript line".into(),
        }),
    );
    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::Message(AgentMessage::Assistant {
            text: "second transcript line".into(),
        }),
    );

    app.editor.agent.scroll = usize::MAX;
    app.render_frame().await;
    let buffer = app.test_buffer_string();

    assert!(
        buffer.contains("first transcript line"),
        "expected scroll clamp to keep earliest transcript line visible\n{buffer}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_transcript_renders_clickable_links() -> anyhow::Result<()> {
    let mut app = AppBuilder::new().build()?;

    open_agent_panel(&mut app);
    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::Message(AgentMessage::Assistant {
            text: "see [link label](README.md) and https://example.com/docs".into(),
        }),
    );

    app.render_frame().await;
    let buffer = app.test_buffer_string();

    assert!(
        buffer.contains("link label"),
        "expected markdown link label in transcript\n{buffer}"
    );
    assert!(
        buffer.contains("https://example.com"),
        "expected bare https URL in transcript\n{buffer}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_permission_request_opens_picker() -> anyhow::Result<()> {
    let mut app = AppBuilder::new().build()?;

    open_agent_panel(&mut app);
    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::PermissionRequested {
            request_id: 1,
            tool_call_id: Some("call-perm".into()),
            title: "Run shell command".into(),
            message: "agent wants to run `cargo test`".into(),
            options: vec![helix_acp::AgentPermissionOption {
                id: "allow-once".into(),
                label: "Allow once".into(),
            }],
        },
    );

    assert!(
        app.editor.agent.pending_permission.is_some(),
        "expected pending permission state"
    );
    assert!(
        app.editor
            .agent
            .permission_gated_tools
            .contains("call-perm"),
        "expected tool to await permission"
    );
    assert!(
        app.editor.agent.open_permission_picker,
        "expected permission picker flag"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_permission_grant_restores_insert_focus() -> anyhow::Result<()> {
    use helix_view::agent::AgentFocus;

    let mut app = AppBuilder::new().build()?;
    open_agent_panel(&mut app);
    app.editor.agent.focus = AgentFocus::Normal;

    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::PermissionRequested {
            request_id: 1,
            tool_call_id: Some("call-perm".into()),
            title: "Run shell command".into(),
            message: "agent wants to run `cargo test`".into(),
            options: vec![helix_acp::AgentPermissionOption {
                id: "allow-once".into(),
                label: "Allow once".into(),
            }],
        },
    );

    helix_term::grant_tool_permission(&mut app.editor, Some("call-perm"));
    helix_term::ui::agent_permission::restore_input_focus_after_permission(&mut app.editor);

    assert_eq!(
        app.editor.agent.focus,
        AgentFocus::Insert,
        "agent input should return to insert mode after permission is resolved"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_debug_event_stores_line_without_transcript_entry() -> anyhow::Result<()> {
    let mut app = AppBuilder::new().build()?;

    open_agent_panel(&mut app);
    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::Debug {
            text: "acp[session/update]: AgentMessageChunk text_len=4".into(),
        },
    );

    assert_eq!(app.editor.agent.debug_log.len(), 1);
    assert!(
        app.editor
            .agent
            .debug_log
            .back()
            .unwrap()
            .contains("AgentMessageChunk"),
        "debug line should be stored"
    );
    assert!(
        app.editor.agent.blocks.is_empty(),
        "debug events should not append transcript entries"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_shell_tool_output_withheld_until_permission_granted() -> anyhow::Result<()> {
    use helix_view::agent::AgentBlockKind;

    let mut app = AppBuilder::new().build()?;
    open_agent_panel(&mut app);

    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::Message(AgentMessage::ToolCall {
            id: "call-git".into(),
            title: "`git status`".into(),
            status: "in_progress".into(),
            detail: Some("On branch main".into()),
            shell_command: Some("git status".into()),
            terminal_id: None,
        }),
    );

    let AgentBlockKind::Tool {
        shell_output,
        detail,
        status,
        ..
    } = &app.editor.agent.blocks[0].kind
    else {
        panic!("expected tool block");
    };
    assert!(shell_output.is_empty(), "output should be withheld");
    assert!(detail.is_none(), "detail should be withheld");
    assert_eq!(status, "awaiting permission");

    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::ToolCallUpdated {
            id: "call-git".into(),
            title: None,
            status: Some("completed".into()),
            detail: Some("modified: agent.rs".into()),
            shell_command: None,
            terminal_id: None,
            agent_output: Some("modified: agent.rs".into()),
        },
    );

    let AgentBlockKind::Tool { shell_output, .. } = &app.editor.agent.blocks[0].kind else {
        panic!("expected tool block");
    };
    assert!(
        shell_output.is_empty(),
        "completed output should stay withheld until permission is granted"
    );

    helix_term::grant_tool_permission(&mut app.editor, Some("call-git"));

    let AgentBlockKind::Tool { shell_output, .. } = &app.editor.agent.blocks[0].kind else {
        panic!("expected tool block");
    };
    assert!(
        shell_output.contains("modified: agent.rs"),
        "output should appear after permission is granted"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_permission_request_withholds_existing_tool_output() -> anyhow::Result<()> {
    use helix_view::agent::AgentBlockKind;

    let mut app = AppBuilder::new().build()?;
    open_agent_panel(&mut app);

    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::Message(AgentMessage::ToolCall {
            id: "call-late".into(),
            title: "Run command".into(),
            status: "in_progress".into(),
            detail: None,
            shell_command: None,
            terminal_id: None,
        }),
    );
    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::ToolCallUpdated {
            id: "call-late".into(),
            title: None,
            status: None,
            detail: None,
            shell_command: None,
            terminal_id: None,
            agent_output: Some("secret output".into()),
        },
    );

    let AgentBlockKind::Tool { shell_output, .. } = &app.editor.agent.blocks[0].kind else {
        panic!("expected tool block");
    };
    assert_eq!(shell_output, "secret output");

    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::PermissionRequested {
            request_id: 2,
            tool_call_id: Some("call-late".into()),
            title: "`git status`".into(),
            message: "allow?".into(),
            options: vec![helix_acp::AgentPermissionOption {
                id: "allow-once".into(),
                label: "Allow once".into(),
            }],
        },
    );

    let AgentBlockKind::Tool { shell_output, status, .. } = &app.editor.agent.blocks[0].kind else {
        panic!("expected tool block");
    };
    assert!(shell_output.is_empty(), "output should be withheld retroactively");
    assert_eq!(status, "awaiting permission");

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_tool_call_updates_merge_by_id() -> anyhow::Result<()> {
    use helix_view::agent::AgentBlockKind;

    let mut app = AppBuilder::new().build()?;

    open_agent_panel(&mut app);
    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::Message(AgentMessage::ToolCall {
            id: "call-1".into(),
            title: "Read file".into(),
            status: "in_progress".into(),
            detail: None,
            shell_command: None,
            terminal_id: None,
        }),
    );
    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::ToolCallUpdated {
            id: "call-1".into(),
            title: None,
            status: Some("completed".into()),
            detail: Some("done".into()),
            shell_command: None,
            terminal_id: None,
            agent_output: None,
        },
    );

    assert_eq!(app.editor.agent.blocks.len(), 1);
    let AgentBlockKind::Tool {
        title,
        status,
        detail,
        expanded,
        ..
    } = &app.editor.agent.blocks[0].kind
    else {
        panic!("expected single tool call block");
    };
    assert_eq!(title, "Read file");
    assert_eq!(status, "completed");
    assert_eq!(detail.as_deref(), Some("done"));
    assert!(!expanded);

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_assistant_chunks_merge_into_one_block() -> anyhow::Result<()> {
    use helix_view::agent::AgentBlockKind;

    let mut app = AppBuilder::new().build()?;
    open_agent_panel(&mut app);

    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::Message(AgentMessage::Assistant {
            text: "first".into(),
        }),
    );
    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::Message(AgentMessage::Assistant {
            text: "second".into(),
        }),
    );

    assert_eq!(app.editor.agent.blocks.len(), 1);
    let AgentBlockKind::Assistant { text } = &app.editor.agent.blocks[0].kind else {
        panic!("expected assistant block");
    };
    assert_eq!(text, "firstsecond");

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn create_plan_opens_plan_panel() -> anyhow::Result<()> {
    let mut app = AppBuilder::new().build()?;

    open_agent_panel(&mut app);
    app.editor.plan.review = Some(helix_view::PlanReview {
        request_id: 1,
        tool_call_id: "call-plan".into(),
        name: Some("Test plan".into()),
        markdown: "# Test plan\n\nDo things.".into(),
    });
    app.editor.open_plan_panel();

    assert!(app.editor.plan.is_open());
    let panel_id = app.editor.plan.panel_id.expect("plan panel id");
    assert!(app.editor.tree.is_plan_panel(panel_id));
    assert!(app.editor.plan.stashed_editor_root.is_some());

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn question_option_advances_flow() -> anyhow::Result<()> {
    let mut app = AppBuilder::new().build()?;

    open_agent_panel(&mut app);
    app.editor.agent.cursor_question_flow = Some(helix_view::agent::AgentQuestionFlow {
        request_id: 2,
        title: Some("Pick one".into()),
        questions: vec![
            helix_view::agent::AgentQuestion {
                id: "q1".into(),
                prompt: "First?".into(),
                options: vec![helix_view::agent::AgentQuestionOption {
                    id: "a".into(),
                    label: "A".into(),
                }],
                allow_multiple: false,
            },
            helix_view::agent::AgentQuestion {
                id: "q2".into(),
                prompt: "Second?".into(),
                options: vec![helix_view::agent::AgentQuestionOption {
                    id: "b".into(),
                    label: "B".into(),
                }],
                allow_multiple: false,
            },
        ],
        answers: Vec::new(),
        index: 0,
    });
    app.editor.open_plan_panel();

    helix_term::ui::plan::select_question_option(&mut app.editor, 0);

    let flow = app
        .editor
        .agent
        .cursor_question_flow
        .as_ref()
        .expect("question flow");
    assert_eq!(flow.index, 1);
    assert_eq!(flow.answers.len(), 1);

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn ask_question_footer_focus_survives_plan_panel_open() -> anyhow::Result<()> {
    let mut app = AppBuilder::new().build()?;

    open_agent_panel(&mut app);
    app.editor.agent.cursor_question_flow = Some(helix_view::agent::AgentQuestionFlow {
        request_id: 3,
        title: Some("Pick one".into()),
        questions: vec![helix_view::agent::AgentQuestion {
            id: "q1".into(),
            prompt: "Which approach?".into(),
            options: vec![helix_view::agent::AgentQuestionOption {
                id: "a".into(),
                label: "Option A".into(),
            }],
            allow_multiple: false,
        }],
        answers: Vec::new(),
        index: 0,
    });
    app.editor.plan.footer_focus = helix_view::plan::PlanFooterFocus::QuestionOption(0);
    app.editor.open_plan_panel();

    assert!(matches!(
        app.editor.plan.footer_focus,
        helix_view::plan::PlanFooterFocus::QuestionOption(0)
    ));

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn plan_accept_in_plan_mode_continues_after_end_turn() -> anyhow::Result<()> {
    let mut app = AppBuilder::new().build()?;

    open_agent_panel(&mut app);
    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::SessionStarted {
            session_id: session_id("session-plan"),
        },
    );
    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::ModeUpdated {
            current_mode: "plan".into(),
            available_modes: vec![helix_acp::AgentModeInfo {
                id: "agent".into(),
                name: "Agent".into(),
                description: None,
            }],
        },
    );
    app.editor.agent.continue_after_plan_accept = true;

    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::TurnFinished {
            stop_reason: Some("EndTurn".into()),
        },
    );

    assert!(!app.editor.agent.continue_after_plan_accept);
    assert_eq!(app.editor.agent.mode.as_deref(), Some("agent"));
    assert!(
        app.editor
            .agent
            .blocks
            .iter()
            .any(|block| matches!(
                &block.kind,
                helix_view::agent::AgentBlockKind::System { text }
                    if text.contains("Continuing to implement")
            )),
        "expected continuation system message"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_session_closed_clears_active_session() -> anyhow::Result<()> {
    let mut app = AppBuilder::new().build()?;

    open_agent_panel(&mut app);
    apply_test_agent_event(
        &mut app.editor,
        AgentEvent::SessionStarted {
            session_id: session_id("session-close"),
        },
    );
    apply_test_agent_event(&mut app.editor, AgentEvent::SessionClosed);

    assert!(app.editor.agent.active_session.is_none());
    assert!(app.editor.agent.mode.is_none());

    Ok(())
}

#[test]
fn sort_agent_sessions_prefers_cwd_and_updated_at() {
    use helix_view::agent::AgentSessionMeta;
    use std::path::PathBuf;

    let cwd = PathBuf::from("/tmp/project");
    let mut sessions = vec![
        AgentSessionMeta {
            id: "old-other".into(),
            title: Some("Old other".into()),
            cwd: PathBuf::from("/elsewhere"),
            updated_at: Some("2026-01-01T00:00:00Z".into()),
        },
        AgentSessionMeta {
            id: "new-local".into(),
            title: Some("New local".into()),
            cwd: cwd.clone(),
            updated_at: Some("2026-01-03T00:00:00Z".into()),
        },
        AgentSessionMeta {
            id: "old-local".into(),
            title: Some("Old local".into()),
            cwd: cwd.clone(),
            updated_at: Some("2026-01-02T00:00:00Z".into()),
        },
    ];

    helix_term::commands::sort_agent_sessions(&mut sessions, Some(&cwd));

    assert_eq!(sessions[0].id, "new-local");
    assert_eq!(sessions[1].id, "old-local");
    assert_eq!(sessions[2].id, "old-other");
}
