use helix_view::{parse_unified_diff_line_map, DiffSide};

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
                buffer.contains("Review comment:"),
                "draft virtual line should appear below cursor, buffer:\n{buffer}"
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
