use std::path::{Path, PathBuf};

use helix_core::Rope;
use helix_view::editor::Action;
use helix_view::Editor;
use helix_vcs::{FileChange, StagingSection};

use crate::compositor::Compositor;
use crate::commands::Context;
use crate::job;
use crate::ui::{self, Prompt, PromptEvent};

pub fn git_panel_toggle(cx: &mut Context) {
    if cx.editor.git.is_open() {
        if cx.editor.tree.is_git_panel(cx.editor.tree.focus) {
            cx.editor.close_git_panel();
        } else {
            cx.editor.focus_git_panel();
            git_refresh(cx);
        }
    } else {
        cx.editor.open_git_panel();
        git_refresh(cx);
    }
}

pub fn git_refresh(cx: &mut Context) {
    schedule_git_refresh(cx.editor);
}

pub fn schedule_git_refresh(editor: &mut Editor) {
    editor.git.loading = true;
    editor.git.error = None;
    let cwd = editor.git_cwd();
    if let Some(head) = editor.diff_providers.get_current_head_name(&cwd) {
        editor.git.branch = Some(head.load().to_string());
    }
    let providers = editor.diff_providers.clone();
    providers.list_status(cwd, |result| {
        job::dispatch_blocking(move |editor, _compositor| match result {
            Ok(entries) => editor.apply_git_status(entries),
            Err(err) => editor.set_git_error(format!("{err:#}")),
        });
    });
}

pub fn git_stage_selected(cx: &mut Context) {
    let Some(entry) = cx.editor.git.selected_entry().cloned() else {
        cx.editor.set_error("no git file selected");
        return;
    };
    if entry.section != StagingSection::Unstaged {
        return;
    }
    git_stage_path(cx, entry.change.path().to_path_buf());
}

pub fn git_stage_path(cx: &mut Context, path: PathBuf) {
    let cwd = cx.editor.git_cwd();
    let providers = cx.editor.diff_providers.clone();
    cx.editor.git.loading = true;
    providers.stage_file(cwd, path.clone(), move |result| {
        job::dispatch_blocking(move |editor, _compositor| match result {
            Ok(()) => {
                editor.set_status(format!("staged {}", path.display()));
                refresh_open_document_diff_bases(editor, &path);
                schedule_git_refresh(editor);
            }
            Err(err) => editor.set_git_error(format!("{err:#}")),
        });
    });
}

pub fn git_stage_all(cx: &mut Context) {
    let cwd = cx.editor.git_cwd();
    let providers = cx.editor.diff_providers.clone();
    cx.editor.git.loading = true;
    providers.stage_all(cwd, move |result| {
        job::dispatch_blocking(move |editor, _compositor| match result {
            Ok(()) => {
                editor.set_status("staged all changes");
                schedule_git_refresh(editor);
            }
            Err(err) => editor.set_git_error(format!("{err:#}")),
        });
    });
}

pub fn git_open(cx: &mut Context) {
    let Some(entry) = cx.editor.git.selected_entry() else {
        cx.editor.set_error("no git file selected");
        return;
    };
    let path = entry.change.path().to_path_buf();
    if matches!(entry.change, FileChange::Deleted { .. }) {
        cx.editor
            .set_error(format!("file deleted: {}", path.display()));
        return;
    }
    cx.editor.focus_editor_from_git();
    if let Err(e) = cx.editor.open(&path, Action::Replace) {
        cx.editor.set_error(format!("{e:#}"));
    }
}

pub fn git_diff(cx: &mut Context) {
    let Some(entry) = cx.editor.git.selected_entry().cloned() else {
        cx.editor.set_error("no git file selected");
        return;
    };
    let path = entry.change.path().to_path_buf();
    let section = entry.section;
    let untracked = matches!(entry.change, FileChange::Untracked { .. });
    let cwd = cx.editor.git_cwd();
    let providers = cx.editor.diff_providers.clone();
    providers.file_diff(cwd, path.clone(), section, untracked, move |result| {
        job::dispatch_blocking(move |editor, _compositor| match result {
            Ok(diff) => open_diff_buffer(editor, &path, &diff),
            Err(err) => editor.set_error(format!("{err:#}")),
        });
    });
}

fn open_diff_buffer(editor: &mut Editor, path: &Path, diff: &str) {
    editor.focus_editor_from_git();
    let config = editor.config.clone();
    let loader = editor.syn_loader.clone();
    let mut doc = helix_view::Document::from(Rope::from(diff), None, config, loader.clone());
    let _ = doc.set_language_by_language_id("diff", &loader.load());
    editor.open_document(doc, Action::VerticalSplit);
    editor.set_status(format!("git diff {}", path.display()));
}

pub fn git_commit_prompt(cx: &mut Context) {
    if cx.editor.git.staged().next().is_none() {
        cx.editor.set_error("nothing staged to commit");
        return;
    }
    let prompt = Prompt::new(
        "Commit message: ".into(),
        None,
        ui::completers::none,
        move |cx, message, event| {
            if event != PromptEvent::Validate {
                return;
            }
            let message = message.trim();
            if message.is_empty() {
                cx.editor.set_error("commit message is empty");
                return;
            }
            let cwd = cx.editor.git_cwd();
            let providers = cx.editor.diff_providers.clone();
            let message = message.to_string();
            cx.editor.git.loading = true;
            providers.commit(cwd, message, move |result| {
                job::dispatch_blocking(move |editor, _compositor| match result {
                    Ok(()) => {
                        editor.set_status("commit created");
                        schedule_git_refresh(editor);
                    }
                    Err(err) => editor.set_git_error(format!("{err:#}")),
                });
            });
        },
    );
    cx.push_layer(Box::new(prompt));
}

pub fn git_commit_prompt_compositor(compositor: &mut Compositor, editor: &mut Editor) {
    if editor.git.staged().next().is_none() {
        editor.set_error("nothing staged to commit");
        return;
    }
    let prompt = Prompt::new(
        "Commit message: ".into(),
        None,
        ui::completers::none,
        move |cx, message, event| {
            if event != PromptEvent::Validate {
                return;
            }
            let message = message.trim();
            if message.is_empty() {
                cx.editor.set_error("commit message is empty");
                return;
            }
            let cwd = cx.editor.git_cwd();
            let providers = cx.editor.diff_providers.clone();
            let message = message.to_string();
            cx.editor.git.loading = true;
            providers.commit(cwd, message, move |result| {
                job::dispatch_blocking(move |editor, _compositor| match result {
                    Ok(()) => {
                        editor.set_status("commit created");
                        schedule_git_refresh(editor);
                    }
                    Err(err) => editor.set_git_error(format!("{err:#}")),
                });
            });
        },
    );
    compositor.push(Box::new(prompt));
}

pub fn git_stage_all_from_compositor(cx: &mut crate::compositor::Context) {
    let cwd = cx.editor.git_cwd();
    let providers = cx.editor.diff_providers.clone();
    cx.editor.git.loading = true;
    providers.stage_all(cwd, move |result| {
        job::dispatch_blocking(move |editor, _compositor| match result {
            Ok(()) => {
                editor.set_status("staged all changes");
                schedule_git_refresh(editor);
            }
            Err(err) => editor.set_git_error(format!("{err:#}")),
        });
    });
}

pub fn git_stage_path_from_compositor(cx: &mut crate::compositor::Context, path: PathBuf) {
    let cwd = cx.editor.git_cwd();
    let providers = cx.editor.diff_providers.clone();
    cx.editor.git.loading = true;
    providers.stage_file(cwd, path.clone(), move |result| {
        job::dispatch_blocking(move |editor, _compositor| match result {
            Ok(()) => {
                editor.set_status(format!("staged {}", path.display()));
                refresh_open_document_diff_bases(editor, &path);
                schedule_git_refresh(editor);
            }
            Err(err) => editor.set_git_error(format!("{err:#}")),
        });
    });
}

pub fn git_focus_editor(cx: &mut Context) {
    cx.editor.focus_editor_from_git();
}

pub fn refresh_open_document_diff_bases(editor: &mut Editor, path: &Path) {
    let base = editor.diff_providers.get_diff_base(path);
    if let Some(base) = base {
        for doc in editor.documents_mut() {
            if doc.path().is_some_and(|p| p.as_path() == path) {
                doc.set_diff_base(base.clone());
            }
        }
    }
}
