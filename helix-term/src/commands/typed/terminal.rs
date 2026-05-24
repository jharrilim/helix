use super::super::prelude::*;
use helix_core::command_line::Args;
pub(crate) fn typed_terminal_open(
    cx: &mut compositor::Context,
    args: Args<'_>,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }
    let cwd = args
        .get_flag("cwd")
        .map(|path| helix_stdx::path::expand_tilde(std::path::Path::new(path)).into_owned());
    crate::commands::terminal::terminal_open_editor(cx.editor, cx.jobs, cwd);
    Ok(())
}

pub(crate) fn typed_terminal_close(
    cx: &mut compositor::Context,
    _args: Args<'_>,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }
    crate::commands::terminal::terminal_close_editor(cx.editor);
    Ok(())
}

pub(crate) fn typed_terminal_new(
    cx: &mut compositor::Context,
    _args: Args<'_>,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }
    crate::commands::terminal::terminal_new_editor(cx.editor, cx.jobs);
    Ok(())
}

pub(crate) fn typed_terminal_list(
    cx: &mut compositor::Context,
    _args: Args<'_>,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }
    let callback = Box::pin(async move {
        let call: crate::job::Callback = crate::job::Callback::EditorCompositor(Box::new(
            |editor, compositor| {
                crate::commands::terminal::show_terminal_list_picker(editor, compositor);
            },
        ));
        Ok(call)
    });
    cx.jobs.callback(callback);
    Ok(())
}

pub(crate) fn typed_terminal_focus(
    cx: &mut compositor::Context,
    args: Args<'_>,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }
    let Some(session_id) = args.first() else {
        cx.editor.set_error("expected terminal session id");
        return Ok(());
    };
    crate::commands::terminal::terminal_focus_editor(cx.editor, session_id);
    Ok(())
}

pub(crate) fn typed_terminal_toggle(
    cx: &mut compositor::Context,
    _args: Args<'_>,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }
    crate::commands::terminal::terminal_toggle_editor(cx.editor, cx.jobs);
    Ok(())
}

pub(crate) fn typed_terminal_send(
    cx: &mut compositor::Context,
    args: Args<'_>,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }
    let append_newline = !args.has_flag("no-newline");
    crate::commands::terminal::terminal_send_editor(cx.editor, append_newline);
    Ok(())
}

