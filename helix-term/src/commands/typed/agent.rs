use super::super::prelude::*;
use helix_core::command_line::Args;
use super::super::Context;
pub(crate) fn typed_agent_open(
    cx: &mut compositor::Context,
    _args: Args<'_>,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }
    crate::commands::agent::agent_open_editor(cx.editor, cx.jobs);
    Ok(())
}

pub(crate) fn typed_agent_close(
    cx: &mut compositor::Context,
    _args: Args<'_>,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }
    crate::commands::agent::agent_close_editor(cx.editor);
    Ok(())
}

pub(crate) fn typed_agent_focus(
    cx: &mut compositor::Context,
    _args: Args<'_>,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }
    crate::commands::agent::agent_focus_editor_panel(cx.editor, cx.jobs);
    Ok(())
}

pub(crate) fn typed_agent_send(
    cx: &mut compositor::Context,
    _args: Args<'_>,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }
    crate::commands::agent::agent_send_editor(cx.editor, cx.jobs);
    Ok(())
}

pub(crate) fn typed_agent_stop(
    cx: &mut compositor::Context,
    _args: Args<'_>,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }
    crate::commands::agent::agent_stop_editor(cx.editor);
    Ok(())
}

pub(crate) fn typed_agent_history(
    cx: &mut compositor::Context,
    args: Args,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }
    let cwd_only = args.has_flag("cwd");
    crate::commands::agent::agent_history_editor(cx.editor, cx.jobs, cwd_only);
    Ok(())
}

pub(crate) fn typed_agent_new(
    cx: &mut compositor::Context,
    _args: Args,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }
    crate::commands::agent::agent_new_editor(cx.editor, cx.jobs);
    Ok(())
}

pub(crate) fn typed_agent_mode(
    cx: &mut compositor::Context,
    args: Args<'_>,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }
    let mode_id = args.first().map(|arg| arg.to_string());
    if let Some(mode_id) = mode_id {
        crate::commands::agent::agent_mode_editor(cx.editor, Some(mode_id));
    } else {
        crate::commands::agent::open_mode_picker_from_jobs(cx.editor, cx.jobs);
    }
    Ok(())
}

