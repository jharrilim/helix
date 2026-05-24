use super::super::prelude::*;
use super::registry::WRITE_NO_FORMAT_FLAG;
use super::lifecycle::{write_impl, WriteOptions};
use helix_core::command_line::Args;
use serde_json::Value;
pub(crate) fn update(cx: &mut compositor::Context, args: Args, event: PromptEvent) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }

    let (_view, doc) = current!(cx.editor);
    if doc.is_modified() {
        write_impl(
            cx,
            None,
            WriteOptions {
                force: false,
                auto_format: !args.has_flag(WRITE_NO_FORMAT_FLAG.name),
            },
        )
    } else {
        Ok(())
    }
}

pub(crate) fn lsp_workspace_command(
    cx: &mut compositor::Context,
    args: Args,
    event: PromptEvent,
) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }

    let doc = doc!(cx.editor);
    let ls_id_commands = doc
        .language_servers_with_feature(LanguageServerFeature::WorkspaceCommand)
        .flat_map(|ls| {
            ls.capabilities()
                .execute_command_provider
                .iter()
                .flat_map(|options| options.commands.iter())
                .map(|command| (ls.id(), command))
        });

    if args.is_empty() {
        let commands = ls_id_commands
            .map(|(ls_id, command)| {
                (
                    ls_id,
                    helix_lsp::lsp::Command {
                        title: command.clone(),
                        command: command.clone(),
                        arguments: None,
                    },
                )
            })
            .collect::<Vec<_>>();
        let callback = async move {
            let call: job::Callback = Callback::EditorCompositor(Box::new(
                move |_editor: &mut Editor, compositor: &mut Compositor| {
                    let columns = [ui::PickerColumn::new(
                        "title",
                        |(_ls_id, command): &(_, helix_lsp::lsp::Command), _| {
                            command.title.as_str().into()
                        },
                    )];
                    let picker = ui::Picker::new(
                        columns,
                        0,
                        commands,
                        (),
                        move |cx, (ls_id, command), _action| {
                            cx.editor.execute_lsp_command(command.clone(), *ls_id);
                        },
                    );
                    compositor.push(Box::new(overlaid(picker)))
                },
            ));
            Ok(call)
        };
        cx.jobs.callback(callback);
    } else {
        let command = args[0].to_string();
        let matches: Vec<_> = ls_id_commands
            .filter(|(_ls_id, c)| *c == &command)
            .collect();

        match matches.as_slice() {
            [(ls_id, _command)] => {
                let arguments = args
                    .get(1)
                    .map(|rest| {
                        serde_json::Deserializer::from_str(rest)
                            .into_iter()
                            .collect::<Result<Vec<Value>, _>>()
                            .map_err(|err| anyhow!("failed to parse arguments: {err}"))
                    })
                    .transpose()?
                    .filter(|args| !args.is_empty());

                cx.editor.execute_lsp_command(
                    helix_lsp::lsp::Command {
                        title: command.clone(),
                        arguments,
                        command,
                    },
                    *ls_id,
                );
            }
            [] => {
                cx.editor.set_status(format!(
                    "`{command}` is not supported for any language server"
                ));
            }
            _ => {
                cx.editor.set_status(format!(
                    "`{command}` supported by multiple language servers"
                ));
            }
        }
    }
    Ok(())
}

pub(crate) fn lsp_restart(cx: &mut compositor::Context, args: Args, event: PromptEvent) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }

    let editor_config = cx.editor.config.load();
    let doc = doc!(cx.editor);
    let config = doc
        .language_config()
        .context("LSP not defined for the current document")?;

    let language_servers: Vec<_> = config
        .language_servers
        .iter()
        .map(|ls| ls.name.as_str())
        .collect();
    let language_servers = if args.is_empty() {
        language_servers
    } else {
        let (valid, invalid): (Vec<_>, Vec<_>) = args
            .iter()
            .map(|arg| arg.as_ref())
            .partition(|name| language_servers.contains(name));
        if !invalid.is_empty() {
            let s = if invalid.len() == 1 { "" } else { "s" };
            bail!("Unknown language server{s}: {}", invalid.join(", "));
        }
        valid
    };

    let mut errors = Vec::new();
    for server in language_servers.iter() {
        match cx
            .editor
            .language_servers
            .restart_server(
                server,
                config,
                doc.path(),
                &editor_config.workspace_lsp_roots,
                editor_config.lsp.snippets,
            )
            .transpose()
        {
            // Ignore the executable-not-found error unless the server was explicitly requested
            // in the arguments.
            Err(helix_lsp::Error::ExecutableNotFound(_))
                if !args.iter().any(|arg| arg == server) => {}
            Err(err) => errors.push(err.to_string()),
            _ => (),
        }
    }

    // This collect is needed because refresh_language_server would need to re-borrow editor.
    let document_ids_to_refresh: Vec<DocumentId> = cx
        .editor
        .documents()
        .filter_map(|doc| match doc.language_config() {
            Some(config)
                if config.language_servers.iter().any(|ls| {
                    language_servers
                        .iter()
                        .any(|restarted_ls| restarted_ls == &ls.name)
                }) =>
            {
                Some(doc.id())
            }
            _ => None,
        })
        .collect();

    for document_id in document_ids_to_refresh {
        cx.editor.refresh_language_servers(document_id);
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "Error restarting language servers: {}",
            errors.join(", ")
        ))
    }
}

pub(crate) fn lsp_stop(cx: &mut compositor::Context, args: Args, event: PromptEvent) -> anyhow::Result<()> {
    if event != PromptEvent::Validate {
        return Ok(());
    }
    let doc = doc!(cx.editor);

    let language_servers: Vec<_> = doc
        .language_servers()
        .map(|ls| ls.name().to_string())
        .collect();
    let language_servers = if args.is_empty() {
        language_servers
    } else {
        let (valid, invalid): (Vec<_>, Vec<_>) = args
            .iter()
            .map(|arg| arg.to_string())
            .partition(|name| language_servers.contains(name));
        if !invalid.is_empty() {
            let s = if invalid.len() == 1 { "" } else { "s" };
            bail!("Unknown language server{s}: {}", invalid.join(", "));
        }
        valid
    };

    for ls_name in &language_servers {
        cx.editor.language_servers.stop(ls_name);

        for doc in cx.editor.documents_mut() {
            if let Some(client) = doc.remove_language_server_by_name(ls_name) {
                doc.clear_diagnostics_for_language_server(client.id());
                doc.reset_all_inlay_hints();
                doc.inlay_hints_oudated = true;
            }
        }
    }

    Ok(())
}

