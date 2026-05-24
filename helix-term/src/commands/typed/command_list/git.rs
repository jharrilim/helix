use std::path::Path;

use crate::commands::typed::{Args, CommandCompleter, PromptEvent, Signature, TypableCommand};
use crate::compositor;
use crate::job::{self, Callback};

pub(super) const COMMANDS: &[TypableCommand] = &[
    TypableCommand {
        name: "git",
        aliases: &["git-status"],
        doc: "Open or refresh the git panel.",
        fun: git_status,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "git-add",
        aliases: &["ga"],
        doc: "Stage files (all if no path given).",
        fun: git_add,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(1)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "git-commit",
        aliases: &["gc"],
        doc: "Commit staged changes.",
        fun: git_commit,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(1)),
            ..Signature::DEFAULT
        },
    },
];

fn git_status(
    cx: &mut compositor::Context,
    _args: Args,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }
    if !cx.editor.git.is_open() {
        cx.editor.open_git_panel();
    } else {
        cx.editor.focus_git_panel();
    }
    crate::commands::git::schedule_git_refresh(cx.editor);
    Ok(())
}

fn git_add(cx: &mut compositor::Context, args: Args, event: PromptEvent) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }
    if args.first().is_none() {
        let cwd = cx.editor.git_cwd();
        let providers = cx.editor.diff_providers.clone();
        cx.editor.git.loading = true;
        providers.stage_all(cwd, move |result| {
            job::dispatch_blocking(move |editor, _compositor| match result {
                Ok(()) => {
                    editor.set_status("staged all changes");
                    crate::commands::git::schedule_git_refresh(editor);
                }
                Err(err) => editor.set_git_error(format!("{err:#}")),
            });
        });
    } else {
        let path = helix_stdx::path::expand_tilde(Path::new(args.first().unwrap())).into_owned();
        let cwd = cx.editor.git_cwd();
        let providers = cx.editor.diff_providers.clone();
        cx.editor.git.loading = true;
        providers.stage_file(cwd, path.clone(), move |result| {
            job::dispatch_blocking(move |editor, _compositor| match result {
                Ok(()) => {
                    editor.set_status(format!("staged {}", path.display()));
                    crate::commands::git::refresh_open_document_diff_bases(editor, &path);
                    crate::commands::git::schedule_git_refresh(editor);
                }
                Err(err) => editor.set_git_error(format!("{err:#}")),
            });
        });
    }
    Ok(())
}

fn git_commit(
    cx: &mut compositor::Context,
    args: Args,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }
    if args.first().is_none() {
        cx.jobs.callback(async move {
            Ok(Callback::EditorCompositor(Box::new(
                |editor, compositor| {
                    crate::commands::git::git_commit_prompt_compositor(compositor, editor);
                },
            )))
        });
    } else {
        let message = args.join(" ");
        let cwd = cx.editor.git_cwd();
        let providers = cx.editor.diff_providers.clone();
        cx.editor.git.loading = true;
        providers.commit(cwd, message, move |result| {
            job::dispatch_blocking(move |editor, _compositor| match result {
                Ok(()) => {
                    editor.set_status("commit created");
                    crate::commands::git::schedule_git_refresh(editor);
                }
                Err(err) => editor.set_git_error(format!("{err:#}")),
            });
        });
    }
    Ok(())
}
