use super::super::prelude::*;
use helix_core::command_line::Args;
use crate::job::Callback;

pub(crate) fn typed_review_toggle(
    cx: &mut compositor::Context,
    _args: Args<'_>,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }
    crate::commands::review::review_toggle_editor(cx.editor);
    Ok(())
}

pub(crate) fn typed_review_comment(
    cx: &mut compositor::Context,
    _args: Args<'_>,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }
    cx.jobs.callback(async move {
        Ok(Callback::EditorCompositor(Box::new(
            |editor, compositor| {
                crate::commands::review::review_comment_compositor(compositor, editor);
            },
        )))
    });
    Ok(())
}

pub(crate) fn typed_review_submit(
    cx: &mut compositor::Context,
    _args: Args<'_>,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }
    crate::commands::review::review_submit_editor(cx.editor, cx.jobs);
    Ok(())
}

pub(crate) fn typed_review_new(
    cx: &mut compositor::Context,
    _args: Args<'_>,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }
    crate::commands::review::review_new_editor(cx.editor);
    Ok(())
}

pub(crate) fn typed_review_list(
    cx: &mut compositor::Context,
    _args: Args<'_>,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }
    cx.jobs.callback(async move {
        Ok(Callback::EditorCompositor(Box::new(
            |editor, compositor| {
                crate::commands::review::review_list_compositor(compositor, editor);
            },
        )))
    });
    Ok(())
}

pub(crate) fn typed_review_summary(
    cx: &mut compositor::Context,
    _args: Args<'_>,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }
    cx.jobs.callback(async move {
        Ok(Callback::EditorCompositor(Box::new(
            |editor, compositor| {
                crate::commands::review::review_summary_compositor(compositor, editor);
            },
        )))
    });
    Ok(())
}

pub(crate) fn typed_review_next(
    cx: &mut compositor::Context,
    _args: Args<'_>,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }
    crate::commands::review::review_next_editor(cx.editor);
    Ok(())
}

pub(crate) fn typed_review_prev(
    cx: &mut compositor::Context,
    _args: Args<'_>,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }
    crate::commands::review::review_prev_editor(cx.editor);
    Ok(())
}

pub(crate) fn typed_review_resume(
    cx: &mut compositor::Context,
    args: Args<'_>,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }
    let review_id = args
        .first()
        .ok_or_else(|| anyhow::anyhow!("review id required"))?;
    crate::commands::review::review_resume_editor(cx.editor, review_id);
    Ok(())
}
