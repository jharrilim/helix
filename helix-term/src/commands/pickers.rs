use super::prelude::*;
use super::{Context, MappableCommand};
use crate::commands::typed;

pub fn file_picker(cx: &mut Context) {
    let root = find_workspace().0;
    if !root.exists() {
        cx.editor.set_error("Workspace directory does not exist");
        return;
    }
    let picker = ui::file_picker(cx.editor, root);
    cx.push_layer(Box::new(overlaid(picker)));
}

pub fn file_picker_in_current_buffer_directory(cx: &mut Context) {
    let doc_dir = doc!(cx.editor)
        .path()
        .and_then(|path| path.parent().map(|path| path.to_path_buf()));

    let path = match doc_dir {
        Some(path) => path,
        None => {
            let cwd = helix_stdx::env::current_working_dir();
            if !cwd.exists() {
                cx.editor.set_error(
                    "Current buffer has no parent and current working directory does not exist",
                );
                return;
            }
            cx.editor.set_error(
                "Current buffer has no parent, opening file picker in current working directory",
            );
            cwd
        }
    };

    let picker = ui::file_picker(cx.editor, path);
    cx.push_layer(Box::new(overlaid(picker)));
}

pub fn file_picker_in_current_directory(cx: &mut Context) {
    let cwd = helix_stdx::env::current_working_dir();
    if !cwd.exists() {
        cx.editor
            .set_error("Current working directory does not exist");
        return;
    }
    let picker = ui::file_picker(cx.editor, cwd);
    cx.push_layer(Box::new(overlaid(picker)));
}

pub fn file_explorer(cx: &mut Context) {
    let root = find_workspace().0;
    if !root.exists() {
        cx.editor.set_error("Workspace directory does not exist");
        return;
    }

    if let Ok(picker) = ui::file_explorer(root, cx.editor) {
        cx.push_layer(Box::new(overlaid(picker)));
    }
}

pub fn file_explorer_in_current_buffer_directory(cx: &mut Context) {
    let doc_dir = doc!(cx.editor)
        .path()
        .and_then(|path| path.parent().map(|path| path.to_path_buf()));

    let path = match doc_dir {
        Some(path) => path,
        None => {
            let cwd = helix_stdx::env::current_working_dir();
            if !cwd.exists() {
                cx.editor.set_error(
                    "Current buffer has no parent and current working directory does not exist",
                );
                return;
            }
            cx.editor.set_error(
                "Current buffer has no parent, opening file explorer in current working directory",
            );
            cwd
        }
    };

    if let Ok(picker) = ui::file_explorer(path, cx.editor) {
        cx.push_layer(Box::new(overlaid(picker)));
    }
}

pub fn file_explorer_in_current_directory(cx: &mut Context) {
    let cwd = helix_stdx::env::current_working_dir();
    if !cwd.exists() {
        cx.editor
            .set_error("Current working directory does not exist");
        return;
    }

    if let Ok(picker) = ui::file_explorer(cwd, cx.editor) {
        cx.push_layer(Box::new(overlaid(picker)));
    }
}

pub fn buffer_picker(cx: &mut Context) {
    let current = view!(cx.editor).doc;

    struct BufferMeta {
        id: DocumentId,
        path: Option<PathBuf>,
        is_modified: bool,
        is_current: bool,
        focused_at: std::time::Instant,
    }

    let new_meta = |doc: &Document| BufferMeta {
        id: doc.id(),
        path: doc.path().cloned(),
        is_modified: doc.is_modified(),
        is_current: doc.id() == current,
        focused_at: doc.focused_at,
    };

    let mut items = cx
        .editor
        .documents
        .values()
        .map(new_meta)
        .collect::<Vec<BufferMeta>>();

    // mru
    items.sort_unstable_by_key(|item| std::cmp::Reverse(item.focused_at));

    let columns = [
        PickerColumn::new("id", |meta: &BufferMeta, _| meta.id.to_string().into()),
        PickerColumn::new("flags", |meta: &BufferMeta, _| {
            let mut flags = String::new();
            if meta.is_modified {
                flags.push('+');
            }
            if meta.is_current {
                flags.push('*');
            }
            flags.into()
        }),
        PickerColumn::new("path", |meta: &BufferMeta, _| {
            let path = meta
                .path
                .as_deref()
                .map(helix_stdx::path::get_relative_path);
            path.as_deref()
                .and_then(Path::to_str)
                .unwrap_or(SCRATCH_BUFFER_NAME)
                .to_string()
                .into()
        }),
    ];

    let initial_cursor = if cx
        .editor
        .config()
        .buffer_picker
        .start_position
        .is_previous()
        && !items.is_empty()
    {
        1
    } else {
        0
    };

    let picker = Picker::new(columns, 2, items, (), |cx, meta, action| {
        cx.editor.switch(meta.id, action);
    })
    .with_initial_cursor(initial_cursor)
    .with_preview(|editor, meta| {
        let doc = &editor.documents.get(&meta.id)?;
        let lines = doc.selections().values().next().map(|selection| {
            let cursor_line = selection.primary().cursor_line(doc.text().slice(..));
            (cursor_line, cursor_line)
        });
        Some((meta.id.into(), lines))
    });
    cx.push_layer(Box::new(overlaid(picker)));
}

pub fn jumplist_picker(cx: &mut Context) {
    struct JumpMeta {
        id: DocumentId,
        path: Option<PathBuf>,
        selection: Selection,
        text: String,
        is_current: bool,
    }

    for (view, _) in cx.editor.tree.views_mut() {
        for doc_id in view.jumps.iter().map(|e| e.0).collect::<Vec<_>>().iter() {
            let doc = doc_mut!(cx.editor, doc_id);
            view.sync_changes(doc);
        }
    }

    let new_meta = |view: &View, doc_id: DocumentId, selection: Selection| {
        let doc = &cx.editor.documents.get(&doc_id);
        let text = doc.map_or("".into(), |d| {
            selection
                .fragments(d.text().slice(..))
                .map(Cow::into_owned)
                .collect::<Vec<_>>()
                .join(" ")
        });

        JumpMeta {
            id: doc_id,
            path: doc.and_then(|d| d.path().cloned()),
            selection,
            text,
            is_current: view.doc == doc_id,
        }
    };

    let columns = [
        ui::PickerColumn::new("id", |item: &JumpMeta, _| item.id.to_string().into()),
        ui::PickerColumn::new("path", |item: &JumpMeta, _| {
            let path = item
                .path
                .as_deref()
                .map(helix_stdx::path::get_relative_path);
            path.as_deref()
                .and_then(Path::to_str)
                .unwrap_or(SCRATCH_BUFFER_NAME)
                .to_string()
                .into()
        }),
        ui::PickerColumn::new("flags", |item: &JumpMeta, _| {
            let mut flags = Vec::new();
            if item.is_current {
                flags.push("*");
            }

            if flags.is_empty() {
                "".into()
            } else {
                format!(" ({})", flags.join("")).into()
            }
        }),
        ui::PickerColumn::new("contents", |item: &JumpMeta, _| item.text.as_str().into()),
    ];

    let picker = Picker::new(
        columns,
        1, // path
        cx.editor.tree.views().flat_map(|(view, _)| {
            view.jumps
                .iter()
                .rev()
                .map(|(doc_id, selection)| new_meta(view, *doc_id, selection.clone()))
        }),
        (),
        |cx, meta, action| {
            cx.editor.switch(meta.id, action);
            let config = cx.editor.config();
            let (view, doc) = (view_mut!(cx.editor), doc_mut!(cx.editor, &meta.id));
            doc.set_selection(view.id, meta.selection.clone());
            if action.align_view(view, doc.id()) {
                view.ensure_cursor_in_view_center(doc, config.scrolloff);
            }
        },
    )
    .with_preview(|editor, meta| {
        let doc = &editor.documents.get(&meta.id)?;
        let line = meta.selection.primary().cursor_line(doc.text().slice(..));
        Some((meta.id.into(), Some((line, line))))
    });
    cx.push_layer(Box::new(overlaid(picker)));
}

pub fn changed_file_picker(cx: &mut Context) {
    pub struct FileChangeData {
        cwd: PathBuf,
        style_untracked: Style,
        style_modified: Style,
        style_conflict: Style,
        style_deleted: Style,
        style_renamed: Style,
    }

    let cwd = helix_stdx::env::current_working_dir();
    if !cwd.exists() {
        cx.editor
            .set_error("Current working directory does not exist");
        return;
    }

    let added = cx.editor.theme.get("diff.plus");
    let modified = cx.editor.theme.get("diff.delta");
    let conflict = cx.editor.theme.get("diff.delta.conflict");
    let deleted = cx.editor.theme.get("diff.minus");
    let renamed = cx.editor.theme.get("diff.delta.moved");

    let columns = [
        PickerColumn::new("change", |change: &FileChange, data: &FileChangeData| {
            match change {
                FileChange::Untracked { .. } => Span::styled("+ untracked", data.style_untracked),
                FileChange::Modified { .. } => Span::styled("~ modified", data.style_modified),
                FileChange::Conflict { .. } => Span::styled("x conflict", data.style_conflict),
                FileChange::Deleted { .. } => Span::styled("- deleted", data.style_deleted),
                FileChange::Renamed { .. } => Span::styled("> renamed", data.style_renamed),
            }
            .into()
        }),
        PickerColumn::new("path", |change: &FileChange, data: &FileChangeData| {
            let display_path = |path: &PathBuf| {
                path.strip_prefix(&data.cwd)
                    .unwrap_or(path)
                    .display()
                    .to_string()
            };
            match change {
                FileChange::Untracked { path } => display_path(path),
                FileChange::Modified { path } => display_path(path),
                FileChange::Conflict { path } => display_path(path),
                FileChange::Deleted { path } => display_path(path),
                FileChange::Renamed { from_path, to_path } => {
                    format!("{} -> {}", display_path(from_path), display_path(to_path))
                }
            }
            .into()
        }),
    ];

    let picker = Picker::new(
        columns,
        1, // path
        [],
        FileChangeData {
            cwd: cwd.clone(),
            style_untracked: added,
            style_modified: modified,
            style_conflict: conflict,
            style_deleted: deleted,
            style_renamed: renamed,
        },
        |cx, meta: &FileChange, action| {
            let path_to_open = meta.path();
            if let Err(e) = cx.editor.open(path_to_open, action) {
                let err = if let Some(err) = e.source() {
                    format!("{}", err)
                } else {
                    format!("unable to open \"{}\"", path_to_open.display())
                };
                cx.editor.set_error(err);
            }
        },
    )
    .with_preview(|_editor, meta| Some((meta.path().into(), None)));
    let injector = picker.injector();

    cx.editor
        .diff_providers
        .clone()
        .for_each_changed_file(cwd, move |change| match change {
            Ok(change) => injector.push(change).is_ok(),
            Err(err) => {
                status::report_blocking(err);
                true
            }
        });
    cx.push_layer(Box::new(overlaid(picker)));
}

pub fn command_palette(cx: &mut Context) {
    let register = cx.register;
    let count = cx.count;

    cx.callback.push(Box::new(
        move |compositor: &mut Compositor, cx: &mut compositor::Context| {
            let keymap = compositor.find::<ui::EditorView>().unwrap().keymaps.map()
                [&cx.editor.mode]
                .reverse_map();

            let commands = MappableCommand::STATIC_COMMAND_LIST.iter().cloned().chain(
                typed::TYPABLE_COMMAND_LIST
                    .iter()
                    .map(|cmd| MappableCommand::Typable {
                        name: cmd.name.to_owned(),
                        args: String::new(),
                        doc: cmd.doc.to_owned(),
                    }),
            );

            let columns = [
                ui::PickerColumn::new("name", |item, _| match item {
                    MappableCommand::Typable { name, .. } => format!(":{name}").into(),
                    MappableCommand::Static { name, .. } => (*name).into(),
                    MappableCommand::Macro { .. } => {
                        unreachable!("macros aren't included in the command palette")
                    }
                }),
                ui::PickerColumn::new(
                    "bindings",
                    |item: &MappableCommand, keymap: &crate::keymap::ReverseKeymap| {
                        keymap
                            .get(item.name())
                            .map(|bindings| {
                                bindings.iter().fold(String::new(), |mut acc, bind| {
                                    if !acc.is_empty() {
                                        acc.push(' ');
                                    }
                                    for key in bind {
                                        acc.push_str(&key.key_sequence_format());
                                    }
                                    acc
                                })
                            })
                            .unwrap_or_default()
                            .into()
                    },
                ),
                ui::PickerColumn::new("doc", |item: &MappableCommand, _| item.doc().into()),
            ];

            let picker = Picker::new(columns, 0, commands, keymap, move |cx, command, _action| {
                let mut ctx = Context {
                    register,
                    count,
                    editor: cx.editor,
                    callback: Vec::new(),
                    on_next_key_callback: None,
                    jobs: cx.jobs,
                };
                let focus = view!(ctx.editor).id;

                command.execute(&mut ctx);

                if ctx.editor.tree.contains(focus) {
                    let config = ctx.editor.config();
                    let mode = ctx.editor.mode();
                    let view = view_mut!(ctx.editor, focus);
                    let doc = doc_mut!(ctx.editor, &view.doc);

                    view.ensure_cursor_in_view(doc, config.scrolloff);

                    if mode != Mode::Insert {
                        doc.append_changes_to_history(view);
                    }
                }
            });
            compositor.push(Box::new(overlaid(picker)));
        },
    ));
}

pub fn last_picker(cx: &mut Context) {
    // TODO: last picker does not seem to work well with buffer_picker
    cx.callback.push(Box::new(|compositor, cx| {
        if let Some(picker) = compositor.last_picker.take() {
            compositor.push(picker);
        } else {
            cx.editor.set_error("no last picker")
        }
    }));
}
