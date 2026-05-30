use helix_term::commands::{add_agent_review_reply_editor, AgentReviewReplyInput};
use helix_view::{
    normalize_comment_path, parse_unified_diff_line_map, timestamp_now, CommentAuthor, DiffSide,
    ReviewComment,
};

use super::helpers::{test_key_sequence, AppBuilder};

#[test]
fn diff_line_map_parses_additions() {
    let diff = "\
--- a/foo.rs
+++ b/foo.rs
@@ -1 +1,2 @@
 line
+added
";
    let map = parse_unified_diff_line_map(std::path::Path::new("/repo/foo.rs"), diff);
    let sides: Vec<_> = map.line_map.iter().flatten().map(|entry| entry.side).collect();
    assert!(sides.contains(&DiffSide::Added));
}

#[tokio::test(flavor = "multi_thread")]
async fn review_comment_prompt_is_visible() -> anyhow::Result<()> {
    use helix_term::job;
    use helix_term::ui::Prompt;

    let mut app = AppBuilder::new().with_file("foo.txt", None).build()?;

    job::dispatch_blocking(|_editor, compositor| {
        compositor.push(Box::new(Prompt::new(
            "Review comment: ".into(),
            None,
            helix_term::ui::completers::none,
            |_, _, _| {},
        )));
    });
    super::helpers::run_event_loop_until_idle(&mut app).await;
    app.render_frame().await;

    let buffer = app.test_buffer_string();
    assert!(
        buffer.contains("Review comment:"),
        "prompt label should render on screen, buffer:\n{buffer}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn review_comment_keymap_opens_prompt() -> anyhow::Result<()> {
    let mut file = tempfile::NamedTempFile::new()?;
    std::io::Write::write_all(&mut file, b"hello\n")?;

    let mut app = AppBuilder::new().with_file(file.path(), None).build()?;

    app.validate_typed_command("review-toggle", "")?;
    test_key_sequence(
        &mut app,
        Some("<space>Rc"),
        Some(&|app| {
            let buffer = app.test_buffer_string();
            assert!(
                buffer.contains("Review comment:"),
                "keymap should open visible review comment prompt, buffer:\n{buffer}"
            );
            assert!(
                buffer.contains("You · Draft"),
                "draft comment box should show author and draft label, buffer:\n{buffer}"
            );
        }),
        false,
    )
    .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn review_comment_typing_updates_prompt_and_draft() -> anyhow::Result<()> {
    let mut file = tempfile::NamedTempFile::new()?;
    std::io::Write::write_all(&mut file, b"hello\n")?;

    let mut app = AppBuilder::new().with_file(file.path(), None).build()?;

    app.validate_typed_command("review-toggle", "")?;
    test_key_sequence(
        &mut app,
        Some("<space>Rchello"),
        Some(&|app| {
            let buffer = app.test_buffer_string();
            assert!(
                buffer.contains("Review comment:"),
                "prompt label should render, buffer:\n{buffer}"
            );
            assert!(
                buffer.contains("hello"),
                "typed text should appear in prompt or draft line, buffer:\n{buffer}"
            );
        }),
        false,
    )
    .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn review_comment_multiline_shift_enter() -> anyhow::Result<()> {
    let mut file = tempfile::NamedTempFile::new()?;
    std::io::Write::write_all(&mut file, b"hello\n")?;

    let mut app = AppBuilder::new().with_file(file.path(), None).build()?;

    app.validate_typed_command("review-toggle", "")?;
    test_key_sequence(
        &mut app,
        Some("<space>Rcline one<S-ret>line two"),
        Some(&|app| {
            let buffer = app.test_buffer_string();
            assert!(
                buffer.contains("line one"),
                "first line should appear in draft box, buffer:\n{buffer}"
            );
            assert!(
                buffer.contains("line two"),
                "second line should appear after shift+enter, buffer:\n{buffer}"
            );
        }),
        false,
    )
    .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn review_comment_enter_shows_footer_buttons() -> anyhow::Result<()> {
    let mut file = tempfile::NamedTempFile::new()?;
    std::io::Write::write_all(&mut file, b"hello\n")?;

    let mut app = AppBuilder::new().with_file(file.path(), None).build()?;

    app.validate_typed_command("review-toggle", "")?;
    test_key_sequence(
        &mut app,
        Some("<space>Rctest<ret>"),
        Some(&|app| {
            let buffer = app.test_buffer_string();
            assert!(
                buffer.contains("Save"),
                "footer should show Save after Enter, buffer:\n{buffer}"
            );
            assert!(
                buffer.contains("Delete"),
                "footer should show Delete after Enter, buffer:\n{buffer}"
            );
        }),
        false,
    )
    .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn typed_review_comment_opens_prompt() -> anyhow::Result<()> {
    let mut app = AppBuilder::new().with_file("foo.txt", None).build()?;

    app.validate_typed_command("review-toggle", "")?;
    test_key_sequence(
        &mut app,
        Some(":review-comment<ret>"),
        Some(&|app| {
            let buffer = app.test_buffer_string();
            assert!(
                buffer.contains("Review comment:"),
                "typed command should open visible review comment prompt, buffer:\n{buffer}"
            );
        }),
        false,
    )
    .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_review_reply_by_comment_id() -> anyhow::Result<()> {
    let mut file = tempfile::NamedTempFile::new()?;
    std::io::Write::write_all(&mut file, b"hello\n")?;
    let file_path = file.path().to_path_buf();

    let mut app = AppBuilder::new().with_file(file.path(), None).build()?;
    app.validate_typed_command("review-toggle", "")?;

    {
        let review = app.editor.review.current.as_mut().expect("active review");
        let normalized = normalize_comment_path(&file_path, &review.metadata.repo_root);
        review.comments.push(ReviewComment {
            id: "parent-comment".into(),
            file: normalized,
            line: 0,
            line_end: None,
            char_idx: 0,
            body: "Existing comment".into(),
            author: CommentAuthor::User,
            context_before: Vec::new(),
            context_after: Vec::new(),
            code_at_comment: String::new(),
            diff_side: None,
            hunk_index: None,
            created_at: timestamp_now(),
        });
    }

    let output = add_agent_review_reply_editor(
        &mut app.editor,
        AgentReviewReplyInput {
            review_id: None,
            comment_id: Some("parent-comment".into()),
            file_path: None,
            line: None,
            body: "Agent reply".into(),
        },
    )
    .map_err(anyhow::Error::msg)?;

    let review = app.editor.review.current.as_ref().expect("active review");
    let inserted = review
        .comments
        .iter()
        .find(|comment| comment.id == output.comment_id)
        .expect("inserted comment");
    assert_eq!(inserted.author, CommentAuthor::Agent);
    assert_eq!(inserted.line, 0);
    assert_eq!(inserted.body, "Agent reply");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_review_reply_by_file_line() -> anyhow::Result<()> {
    let mut file = tempfile::NamedTempFile::new()?;
    std::io::Write::write_all(&mut file, b"hello\n")?;
    let file_path = file.path().to_path_buf();

    let mut app = AppBuilder::new().with_file(file.path(), None).build()?;
    app.validate_typed_command("review-toggle", "")?;

    let output = add_agent_review_reply_editor(
        &mut app.editor,
        AgentReviewReplyInput {
            review_id: None,
            comment_id: None,
            file_path: Some(file_path.clone()),
            line: Some(0),
            body: "Agent line reply".into(),
        },
    )
    .map_err(anyhow::Error::msg)?;

    let review = app.editor.review.current.as_ref().expect("active review");
    let inserted = review
        .comments
        .iter()
        .find(|comment| comment.id == output.comment_id)
        .expect("inserted comment");
    assert_eq!(inserted.author, CommentAuthor::Agent);
    assert_eq!(inserted.line, 0);
    let normalized = normalize_comment_path(&file_path, &review.metadata.repo_root);
    assert_eq!(inserted.file, normalized);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_review_reply_invalid_comment_id_errors() -> anyhow::Result<()> {
    let mut app = AppBuilder::new().with_file("foo.txt", None).build()?;
    app.validate_typed_command("review-toggle", "")?;

    let err = add_agent_review_reply_editor(
        &mut app.editor,
        AgentReviewReplyInput {
            review_id: None,
            comment_id: Some("missing-comment".into()),
            file_path: None,
            line: None,
            body: "reply".into(),
        },
    )
    .expect_err("missing comment id should fail");
    assert!(err.contains("not found"));
    Ok(())
}
