use std::io;
use std::path::Path;

use helix_loader::workspace_trust::TrustStatus;
use helix_lsp::LanguageServerId;
use helix_stdx::path::canonicalize;

use crate::DocumentId;

use super::Editor;

impl Editor {
    #[inline]
    pub fn language_server_by_id(
        &self,
        language_server_id: LanguageServerId,
    ) -> Option<&helix_lsp::Client> {
        self.language_servers
            .get_by_id(language_server_id)
            .map(|client| &**client)
    }

    /// Refreshes the language server for a given document
    pub fn refresh_language_servers(&mut self, doc_id: DocumentId) {
        self.launch_language_servers(doc_id)
    }

    /// moves/renames a path, invoking any event handlers (currently only lsp)
    /// and calling `set_doc_path` if the file is open in the editor
    pub fn move_path(&mut self, old_path: &Path, new_path: &Path) -> io::Result<()> {
        let new_path = canonicalize(new_path);
        // sanity check
        if old_path == new_path {
            return Ok(());
        }
        let is_dir = old_path.is_dir();
        let language_servers: Vec<_> = self
            .language_servers
            .iter_clients()
            .filter(|client| client.is_initialized())
            .cloned()
            .collect();
        for language_server in language_servers {
            let Some(request) = language_server.will_rename(old_path, &new_path, is_dir) else {
                continue;
            };
            let edit = match helix_lsp::block_on(request) {
                Ok(edit) => edit.unwrap_or_default(),
                Err(err) => {
                    log::error!("invalid willRename response: {err:?}");
                    continue;
                }
            };
            if let Err(err) = self.apply_workspace_edit(language_server.offset_encoding(), &edit) {
                log::error!("failed to apply workspace edit: {err:?}")
            }
        }

        if old_path.exists() {
            std::fs::rename(old_path, &new_path)?;
        }

        if let Some(doc) = self.document_by_path(old_path) {
            self.set_doc_path(doc.id(), &new_path);
        }
        let is_dir = new_path.is_dir();
        for ls in self.language_servers.iter_clients() {
            // A new language server might have been started in `set_doc_path` and won't
            // be initialized yet. Skip the `did_rename` notification for this server.
            if !ls.is_initialized() {
                continue;
            }
            ls.did_rename(old_path, &new_path, is_dir);
        }
        self.language_servers
            .file_event_handler
            .file_changed(old_path.to_owned());
        self.language_servers
            .file_event_handler
            .file_changed(new_path);
        Ok(())
    }

    pub fn set_doc_path(&mut self, doc_id: DocumentId, path: &Path) {
        let doc = doc_mut!(self, &doc_id);
        let old_path = doc.path();

        if let Some(old_path) = old_path {
            // sanity check, should not occur but some callers (like an LSP) may
            // create bogus calls
            if old_path == path {
                return;
            }
            // if we are open in LSPs send did_close notification
            for language_server in doc.language_servers() {
                language_server.text_document_did_close(doc.identifier());
            }
        }
        // we need to clear the list of language servers here so that
        // refresh_doc_language/refresh_language_servers doesn't resend
        // text_document_did_close. Since we called `text_document_did_close`
        // we have fully unregistered this document from its LS
        doc.language_servers.clear();
        doc.set_path(Some(path));
        doc.detect_editor_config();
        self.refresh_doc_language(doc_id)
    }

    pub fn refresh_doc_language(&mut self, doc_id: DocumentId) {
        let loader = self.syn_loader.load();
        let doc = doc_mut!(self, &doc_id);
        doc.detect_language(&loader);
        doc.detect_editor_config();
        doc.detect_indent_and_line_ending();
        self.refresh_language_servers(doc_id);
        let doc = doc_mut!(self, &doc_id);
        let diagnostics = Editor::doc_diagnostics(&self.language_servers, &self.diagnostics, doc);
        doc.replace_diagnostics(diagnostics, &[], None);
        doc.reset_all_inlay_hints();
    }

    /// Launch a language server for a given document
    pub fn launch_language_servers(&mut self, doc_id: DocumentId) {
        if !self.config().lsp.enable {
            return;
        }
        // if doc doesn't have a URL it's a scratch buffer, ignore it
        let Some(doc) = self.documents.get_mut(&doc_id) else {
            return;
        };
        let Some(doc_url) = doc.url() else {
            return;
        };
        let (lang, path) = (doc.language.clone(), doc.path().cloned());
        let config = doc.config.load();
        let root_dirs = &config.workspace_lsp_roots;

        if let TrustStatus::Untrusted =
            helix_loader::workspace_trust::quick_query_workspace(self.config.load().insecure)
        {
            self.set_status(
                "Current workspace is not trusted. Run `:workspace-trust` to enable all features.",
            );
            return;
        };

        // store only successfully started language servers
        let language_servers = lang.as_ref().map_or_else(std::collections::HashMap::default, |language| {
            self.language_servers
                .get(language, path.as_ref(), root_dirs, config.lsp.snippets)
                .filter_map(|(lang, client)| match client {
                    Ok(client) => Some((lang, client)),
                    Err(err) => {
                        if let helix_lsp::Error::ExecutableNotFound(err) = err {
                            // Silence by default since some language servers might just not be installed
                            log::debug!(
                                "Language server not found for `{}` {} {}", language.scope, lang, err,
                            );
                        } else {
                            log::error!(
                                "Failed to initialize the language servers for `{}` - `{}` {{ {} }}",
                                language.scope,
                                lang,
                                err
                            );
                        }
                        None
                    }
                })
                .collect::<std::collections::HashMap<_, _>>()
        });

        if language_servers.is_empty() && doc.language_servers.is_empty() {
            return;
        }

        let language_id = doc.language_id().map(ToOwned::to_owned).unwrap_or_default();

        // only spawn new language servers if the servers aren't the same
        let doc_language_servers_not_in_registry =
            doc.language_servers.iter().filter(|(name, doc_ls)| {
                language_servers
                    .get(*name)
                    .is_none_or(|ls| ls.id() != doc_ls.id())
            });

        for (_, language_server) in doc_language_servers_not_in_registry {
            language_server.text_document_did_close(doc.identifier());
        }

        let language_servers_not_in_doc = language_servers.iter().filter(|(name, ls)| {
            doc.language_servers
                .get(*name)
                .is_none_or(|doc_ls| ls.id() != doc_ls.id())
        });

        for (_, language_server) in language_servers_not_in_doc {
            // TODO: this now races with on_init code if the init happens too quickly
            language_server.text_document_did_open(
                doc_url.clone(),
                doc.version(),
                doc.text(),
                language_id.clone(),
            );
        }

        doc.language_servers = language_servers;
    }
}
