use crate::job::Job;
use super::super::prelude::*;
use super::buffer::{buffer_close_by_ids_impl, buffer_gather_paths_impl};
use super::registry::WRITE_NO_FORMAT_FLAG;
use super::super::make_format_callback;
use helix_core::command_line::Args;
use helix_core::line_ending;
pub(crate) fn exit(cx: &mut compositor::Context, args: Args, event: PromptEvent) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }

    if let Some((_, doc)) = helix_view::try_current_ref!(cx.editor) {
        if doc.is_modified() {
            write_impl(
                cx,
                args.first(),
                WriteOptions {
                    force: false,
                    auto_format: !args.has_flag(WRITE_NO_FORMAT_FLAG.name),
                },
            )?;
        }
    }
    cx.block_try_flush_writes()?;
    quit(cx, Args::default(), event)
}

pub(crate) fn force_exit(cx: &mut compositor::Context, args: Args, event: PromptEvent) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }

    if let Some((_, doc)) = helix_view::try_current_ref!(cx.editor) {
        if doc.is_modified() {
            write_impl(
                cx,
                args.first(),
                WriteOptions {
                    force: true,
                    auto_format: !args.has_flag(WRITE_NO_FORMAT_FLAG.name),
                },
            )?;
        }
    }
    cx.block_try_flush_writes()?;
    quit(cx, Args::default(), event)
}

pub(crate) fn quit(cx: &mut compositor::Context, _args: Args, event: PromptEvent) -> anyhow::Result<()> {
    log::debug!("quitting...");

    if event != PromptEvent::Validate {
        return Ok(());
    }

    cx.block_try_flush_writes()?;
    if cx.editor.tree.is_agent_panel(cx.editor.tree.focus) {
        crate::commands::agent::close_agent_panel_editor(cx.editor);
        return Ok(());
    }
    if cx.editor.tree.is_terminal_panel(cx.editor.tree.focus) {
        crate::commands::terminal::close_terminal_panel_editor(cx.editor);
        return Ok(());
    }
    if cx.editor.tree.is_git_panel(cx.editor.tree.focus) {
        cx.editor.close_git_panel();
        return Ok(());
    }
    if cx.editor.tree.is_plan_panel(cx.editor.tree.focus) {
        crate::ui::plan::cancel_plan_flow(cx.editor);
        return Ok(());
    }

    // last view and we have unsaved changes
    if cx.editor.tree.views().count() == 1 {
        buffers_remaining_impl(cx.editor)?
    }

    if !cx.editor.is_document_view_focused() {
        cx.editor.set_error("command requires a document view");
        return Ok(());
    }

    cx.editor.close(view!(cx.editor).id);

    Ok(())
}

pub(crate) fn force_quit(cx: &mut compositor::Context, _args: Args, event: PromptEvent) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }

    cx.block_try_flush_writes()?;
    if cx.editor.tree.is_agent_panel(cx.editor.tree.focus) {
        crate::commands::agent::close_agent_panel_editor(cx.editor);
        return Ok(());
    }
    if cx.editor.tree.is_terminal_panel(cx.editor.tree.focus) {
        crate::commands::terminal::close_terminal_panel_editor(cx.editor);
        return Ok(());
    }
    if cx.editor.tree.is_git_panel(cx.editor.tree.focus) {
        cx.editor.close_git_panel();
        return Ok(());
    }
    if cx.editor.tree.is_plan_panel(cx.editor.tree.focus) {
        crate::ui::plan::cancel_plan_flow(cx.editor);
        return Ok(());
    }

    if !cx.editor.is_document_view_focused() {
        cx.editor.set_error("command requires a document view");
        return Ok(());
    }

    cx.editor.close(view!(cx.editor).id);

    Ok(())
}


pub(crate) fn write_impl(
    cx: &mut compositor::Context,
    path: Option<&str>,
    options: WriteOptions,
) -> anyhow::Result<()> {
    if !crate::commands::require_document_view_or_error(cx.editor) {
        return Ok(());
    }

    let config = cx.editor.config();
    let jobs = &mut cx.jobs;
    let (view, doc) = current!(cx.editor);

    if doc.trim_trailing_whitespace() {
        trim_trailing_whitespace(doc, view.id);
    }
    if config.trim_final_newlines {
        trim_final_newlines(doc, view.id);
    }
    if doc.insert_final_newline() {
        insert_final_newline(doc, view.id);
    }

    // Save an undo checkpoint for any outstanding changes.
    doc.append_changes_to_history(view);

    let (view, doc) = current_ref!(cx.editor);
    let fmt = if config.auto_format && options.auto_format {
        doc.auto_format(cx.editor).map(|fmt| {
            let callback = make_format_callback(
                doc.id(),
                doc.version(),
                view.id,
                fmt,
                Some((path.map(Into::into), options.force)),
            );

            jobs.add(Job::with_callback(callback).wait_before_exiting());
        })
    } else {
        None
    };

    if fmt.is_none() {
        let id = doc.id();
        cx.editor.save(id, path, options.force)?;
    }

    Ok(())
}

/// Trim all whitespace preceding line-endings in a document.
fn trim_trailing_whitespace(doc: &mut Document, view_id: ViewId) {
    let text = doc.text();
    let mut pos = 0;
    let transaction = Transaction::delete(
        text,
        text.lines().filter_map(|line| {
            let line_end_len_chars = line_ending::get_line_ending(&line)
                .map(|le| le.len_chars())
                .unwrap_or_default();
            // Char after the last non-whitespace character or the beginning of the line if the
            // line is all whitespace:
            let first_trailing_whitespace =
                pos + line.last_non_whitespace_char().map_or(0, |idx| idx + 1);
            pos += line.len_chars();
            // Char before the line ending character(s), or the final char in the text if there
            // is no line-ending on this line:
            let line_end = pos - line_end_len_chars;
            if first_trailing_whitespace != line_end {
                Some((first_trailing_whitespace, line_end))
            } else {
                None
            }
        }),
    );
    doc.apply(&transaction, view_id);
}

/// Trim any extra line-endings after the final line-ending.
fn trim_final_newlines(doc: &mut Document, view_id: ViewId) {
    let rope = doc.text();
    let mut text = rope.slice(..);
    let mut total_char_len = 0;
    let mut final_char_len = 0;
    while let Some(line_ending) = line_ending::get_line_ending(&text) {
        total_char_len += line_ending.len_chars();
        final_char_len = line_ending.len_chars();
        text = text.slice(..text.len_chars() - line_ending.len_chars());
    }
    let chars_to_delete = total_char_len - final_char_len;
    if chars_to_delete != 0 {
        let transaction = Transaction::delete(
            rope,
            [(rope.len_chars() - chars_to_delete, rope.len_chars())].into_iter(),
        );
        doc.apply(&transaction, view_id);
    }
}

/// Ensure that the document is terminated with a line ending.
fn insert_final_newline(doc: &mut Document, view_id: ViewId) {
    let text = doc.text();
    if text.len_chars() > 0 && line_ending::get_line_ending(&text.slice(..)).is_none() {
        let eof = Selection::point(text.len_chars());
        let insert = Transaction::insert(text, &eof, doc.line_ending.as_str().into());
        doc.apply(&insert, view_id);
    }
}

#[derive(Debug, Clone, Copy)]
pub struct WriteOptions {
    pub force: bool,
    pub auto_format: bool,
}

pub(crate) fn write(cx: &mut compositor::Context, args: Args, event: PromptEvent) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }

    write_impl(
        cx,
        args.first(),
        WriteOptions {
            force: false,
            auto_format: !args.has_flag(WRITE_NO_FORMAT_FLAG.name),
        },
    )
}

pub(crate) fn force_write(cx: &mut compositor::Context, args: Args, event: PromptEvent) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }

    write_impl(
        cx,
        args.first(),
        WriteOptions {
            force: true,
            auto_format: !args.has_flag(WRITE_NO_FORMAT_FLAG.name),
        },
    )
}

pub(crate) fn write_buffer_close(
    cx: &mut compositor::Context,
    args: Args,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }

    write_impl(
        cx,
        args.first(),
        WriteOptions {
            force: false,
            auto_format: !args.has_flag(WRITE_NO_FORMAT_FLAG.name),
        },
    )?;

    let document_ids = buffer_gather_paths_impl(cx.editor, args);
    buffer_close_by_ids_impl(cx, &document_ids, false)
}

pub(crate) fn force_write_buffer_close(
    cx: &mut compositor::Context,
    args: Args,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }

    write_impl(
        cx,
        args.first(),
        WriteOptions {
            force: true,
            auto_format: !args.has_flag(WRITE_NO_FORMAT_FLAG.name),
        },
    )?;

    let document_ids = buffer_gather_paths_impl(cx.editor, args);
    buffer_close_by_ids_impl(cx, &document_ids, false)
}

pub(crate) fn write_quit(cx: &mut compositor::Context, args: Args, event: PromptEvent) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }

    write_impl(
        cx,
        args.first(),
        WriteOptions {
            force: false,
            auto_format: !args.has_flag(WRITE_NO_FORMAT_FLAG.name),
        },
    )?;
    cx.block_try_flush_writes()?;
    quit(cx, Args::default(), event)
}

pub(crate) fn force_write_quit(
    cx: &mut compositor::Context,
    args: Args,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }

    write_impl(
        cx,
        args.first(),
        WriteOptions {
            force: true,
            auto_format: !args.has_flag(WRITE_NO_FORMAT_FLAG.name),
        },
    )?;
    cx.block_try_flush_writes()?;
    force_quit(cx, Args::default(), event)
}

/// Results in an error if there are modified buffers remaining and sets editor
/// error, otherwise returns `Ok(())`. If the current document is unmodified,
/// and there are modified documents, switches focus to one of them.
pub(crate) fn buffers_remaining_impl(editor: &mut Editor) -> anyhow::Result<()> {
    let modified_ids: Vec<_> = editor
        .documents()
        .filter(|doc| doc.is_modified())
        .map(|doc| doc.id())
        .collect();

    if let Some(first) = modified_ids.first() {
        if let Some((_, current)) = helix_view::try_current_ref!(editor) {
            // If the current document is unmodified, and there are modified
            // documents, switch focus to the first modified doc.
            if !modified_ids.contains(&current.id()) {
                editor.switch(*first, Action::Replace);
            }
        }

        let modified_names: Vec<_> = modified_ids
            .iter()
            .map(|doc_id| doc!(editor, doc_id).display_name())
            .collect();

        bail!(
            "{} unsaved buffer{} remaining: {:?}",
            modified_names.len(),
            if modified_names.len() == 1 { "" } else { "s" },
            modified_names,
        );
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub struct WriteAllOptions {
    pub force: bool,
    pub write_scratch: bool,
    pub auto_format: bool,
}

pub fn write_all_impl(
    cx: &mut compositor::Context,
    options: WriteAllOptions,
) -> anyhow::Result<()> {
    let mut errors: Vec<&'static str> = Vec::new();
    let config = cx.editor.config();
    let jobs = &mut cx.jobs;
    let saves: Vec<_> = cx
        .editor
        .documents
        .keys()
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
        .filter_map(|id| {
            let doc = doc!(cx.editor, &id);
            if !doc.is_modified() {
                return None;
            }
            if doc.path().is_none() {
                if options.write_scratch {
                    errors.push("cannot write a buffer without a filename");
                }
                return None;
            }

            // Look for a view to apply the formatting change to.
            let target_view = cx.editor.get_synced_view_id(doc.id());
            Some((id, target_view))
        })
        .collect();

    for (doc_id, target_view) in saves {
        let doc = doc_mut!(cx.editor, &doc_id);
        let view = view_mut!(cx.editor, target_view);

        if doc.trim_trailing_whitespace() {
            trim_trailing_whitespace(doc, target_view);
        }
        if config.trim_final_newlines {
            trim_final_newlines(doc, target_view);
        }
        if doc.insert_final_newline() {
            insert_final_newline(doc, target_view);
        }

        // Save an undo checkpoint for any outstanding changes.
        doc.append_changes_to_history(view);

        let fmt = if options.auto_format && config.auto_format {
            let doc = doc!(cx.editor, &doc_id);
            doc.auto_format(cx.editor).map(|fmt| {
                let callback = make_format_callback(
                    doc_id,
                    doc.version(),
                    target_view,
                    fmt,
                    Some((None, options.force)),
                );
                jobs.add(Job::with_callback(callback).wait_before_exiting());
            })
        } else {
            None
        };

        if fmt.is_none() {
            cx.editor.save::<PathBuf>(doc_id, None, options.force)?;
        }
    }

    if !errors.is_empty() && !options.force {
        bail!("{:?}", errors);
    }

    Ok(())
}

pub(crate) fn write_all(cx: &mut compositor::Context, args: Args, event: PromptEvent) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }

    write_all_impl(
        cx,
        WriteAllOptions {
            force: false,
            write_scratch: true,
            auto_format: !args.has_flag(WRITE_NO_FORMAT_FLAG.name),
        },
    )
}

pub(crate) fn force_write_all(
    cx: &mut compositor::Context,
    args: Args,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }

    write_all_impl(
        cx,
        WriteAllOptions {
            force: true,
            write_scratch: true,
            auto_format: !args.has_flag(WRITE_NO_FORMAT_FLAG.name),
        },
    )
}

pub(crate) fn write_all_quit(
    cx: &mut compositor::Context,
    args: Args,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }
    write_all_impl(
        cx,
        WriteAllOptions {
            force: false,
            write_scratch: true,
            auto_format: !args.has_flag(WRITE_NO_FORMAT_FLAG.name),
        },
    )?;
    quit_all_impl(cx, false)
}

pub(crate) fn force_write_all_quit(
    cx: &mut compositor::Context,
    args: Args,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }
    let _ = write_all_impl(
        cx,
        WriteAllOptions {
            force: true,
            write_scratch: true,
            auto_format: !args.has_flag(WRITE_NO_FORMAT_FLAG.name),
        },
    );
    quit_all_impl(cx, true)
}

fn quit_all_impl(cx: &mut compositor::Context, force: bool) -> anyhow::Result<()> {
    cx.block_try_flush_writes()?;
    if !force {
        buffers_remaining_impl(cx.editor)?;
    }

    crate::commands::agent::close_agent_panel_editor(cx.editor);
    crate::commands::terminal::close_terminal_panel_editor(cx.editor);
    cx.editor.close_git_panel();

    // close all views
    let views: Vec<_> = cx.editor.tree.views().map(|(view, _)| view.id).collect();
    for view_id in views {
        cx.editor.close(view_id);
    }

    Ok(())
}

pub(crate) fn quit_all(cx: &mut compositor::Context, _args: Args, event: PromptEvent) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }

    quit_all_impl(cx, false)
}

pub(crate) fn force_quit_all(
    cx: &mut compositor::Context,
    _args: Args,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }

    quit_all_impl(cx, true)
}

pub(crate) fn cquit(cx: &mut compositor::Context, args: Args, event: PromptEvent) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }

    let exit_code = args
        .first()
        .and_then(|code| code.parse::<i32>().ok())
        .unwrap_or(1);

    cx.editor.exit_code = exit_code;
    quit_all_impl(cx, false)
}

pub(crate) fn force_cquit(cx: &mut compositor::Context, args: Args, event: PromptEvent) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }

    let exit_code = args
        .first()
        .and_then(|code| code.parse::<i32>().ok())
        .unwrap_or(1);
    cx.editor.exit_code = exit_code;

    quit_all_impl(cx, true)
}

