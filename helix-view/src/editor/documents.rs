use std::io::stdin;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Error};
use helix_event::dispatch;
use helix_stdx::path::canonicalize;
use futures_util::StreamExt;
use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::{
    events::{DocumentDidClose, DocumentDidOpen, DocumentFocusLost},
    Document, DocumentId, View, ViewId,
};

use super::{Action, CloseError, Editor};

impl Editor {
    pub(crate) fn replace_document_in_view(&mut self, current_view: ViewId, doc_id: DocumentId) {
        let scrolloff = self.config().scrolloff;
        let view = self.tree.get_mut(current_view);

        view.doc = doc_id;
        let doc = doc_mut!(self, &doc_id);

        doc.ensure_view_init(view.id);
        view.sync_changes(doc);
        doc.mark_as_focused();

        view.ensure_cursor_in_view(doc, scrolloff)
    }

    pub fn switch(&mut self, id: DocumentId, action: Action) {
        use crate::tree::Layout;

        if !self.documents.contains_key(&id) {
            log::error!("cannot switch to document that does not exist (anymore)");
            return;
        }

        if !matches!(action, Action::Load) {
            self.enter_normal_mode();
        }

        let focust_lost = match action {
            Action::Replace => {
                let (view, doc) = current_ref!(self);
                // If the current view is an empty scratch buffer and is not displayed in any other views, delete it.
                // Boolean value is determined before the call to `view_mut` because the operation requires a borrow
                // of `self.tree`, which is mutably borrowed when `view_mut` is called.
                let remove_empty_scratch = !doc.is_modified()
                    // If the buffer has no path and is not modified, it is an empty scratch buffer.
                    && doc.path().is_none()
                    // If the buffer we are changing to is not this buffer
                    && id != doc.id
                    // Ensure the buffer is not displayed in any other splits.
                    && !self
                        .tree
                        .traverse()
                        .any(|(_, v)| v.doc == doc.id && v.id != view.id);

                let (view, doc) = current!(self);
                let view_id = view.id;

                // Append any outstanding changes to history in the old document.
                doc.append_changes_to_history(view);

                if remove_empty_scratch {
                    // Copy `doc.id` into a variable before calling `self.documents.remove`, which requires a mutable
                    // borrow, invalidating direct access to `doc.id`.
                    let id = doc.id;
                    self.documents.remove(&id);

                    // Remove the scratch buffer from any jumplists
                    for (view, _) in self.tree.views_mut() {
                        view.remove_document(&id);
                    }
                } else {
                    let jump = (view.doc, doc.selection(view.id).clone());
                    view.jumps.push(jump);
                    // Set last accessed doc if it is a different document
                    if doc.id != id {
                        view.add_to_history(view.doc);
                        // Set last modified doc if modified and last modified doc is different
                        if std::mem::take(&mut doc.modified_since_accessed)
                            && view.last_modified_docs[0] != Some(view.doc)
                        {
                            view.last_modified_docs = [Some(view.doc), view.last_modified_docs[0]];
                        }
                    }
                }

                self.replace_document_in_view(view_id, id);

                dispatch(DocumentFocusLost {
                    editor: self,
                    doc: id,
                });
                return;
            }
            Action::Load => {
                let view_id = view!(self).id;
                let doc = doc_mut!(self, &id);
                doc.ensure_view_init(view_id);
                doc.mark_as_focused();
                return;
            }
            Action::HorizontalSplit | Action::VerticalSplit => {
                let focus_lost = self.tree.try_get(self.tree.focus).map(|view| view.doc);
                // copy the current view, unless there is no view yet
                let view = self
                    .tree
                    .try_get(self.tree.focus)
                    .filter(|v| id == v.doc) // Different Document
                    .cloned()
                    .unwrap_or_else(|| View::new(id, self.config().gutters.clone()));
                let view_id = self.tree.split(
                    view,
                    match action {
                        Action::HorizontalSplit => Layout::Horizontal,
                        Action::VerticalSplit => Layout::Vertical,
                        _ => unreachable!(),
                    },
                );
                // initialize selection for view
                let doc = doc_mut!(self, &id);
                doc.ensure_view_init(view_id);
                doc.mark_as_focused();
                focus_lost
            }
        };

        self._refresh();
        if let Some(focus_lost) = focust_lost {
            dispatch(DocumentFocusLost {
                editor: self,
                doc: focus_lost,
            });
        }
    }

    /// Generate an id for a new document and register it.
    fn new_document(&mut self, mut doc: Document) -> DocumentId {
        let id = self.next_document_id;
        // Safety: adding 1 from 1 is fine, practically impossible to reach usize max
        self.next_document_id =
            DocumentId(unsafe { NonZeroUsize::new_unchecked(self.next_document_id.0.get() + 1) });
        doc.id = id;
        self.documents.insert(id, doc);

        let (save_sender, save_receiver) = tokio::sync::mpsc::unbounded_channel();
        self.saves.insert(id, save_sender);

        let stream = UnboundedReceiverStream::new(save_receiver).flatten();
        self.save_queue.push(stream);

        id
    }

    fn new_file_from_document(&mut self, action: Action, doc: Document) -> DocumentId {
        let id = self.new_document(doc);
        self.switch(id, action);
        id
    }

    pub fn new_file(&mut self, action: Action) -> DocumentId {
        self.new_file_from_document(
            action,
            Document::default(self.config.clone(), self.syn_loader.clone()),
        )
    }

    pub fn new_file_from_stdin(&mut self, action: Action) -> Result<DocumentId, Error> {
        let (stdin, encoding, has_bom) = crate::document::read_to_string(&mut stdin(), None)?;
        let doc = Document::from(
            helix_core::Rope::default(),
            Some((encoding, has_bom)),
            self.config.clone(),
            self.syn_loader.clone(),
        );
        let doc_id = self.new_file_from_document(action, doc);
        let doc = doc_mut!(self, &doc_id);
        let view = view_mut!(self);
        doc.ensure_view_init(view.id);
        let transaction =
            helix_core::Transaction::insert(doc.text(), doc.selection(view.id), stdin.into())
                .with_selection(helix_core::Selection::point(0));
        doc.apply(&transaction, view.id);
        doc.append_changes_to_history(view);
        Ok(doc_id)
    }

    pub fn document_id_by_path(&self, path: &Path) -> Option<DocumentId> {
        self.document_by_path(path).map(|doc| doc.id)
    }

    // ??? possible use for integration tests
    pub fn open(&mut self, path: &Path, action: Action) -> Result<DocumentId, crate::document::DocumentOpenError> {
        let path = canonicalize(path);
        let id = self.document_id_by_path(&path);

        let id = if let Some(id) = id {
            id
        } else {
            let mut doc = Document::open(
                &path,
                None,
                true,
                self.config.clone(),
                self.syn_loader.clone(),
            )?;

            let diagnostics =
                Editor::doc_diagnostics(&self.language_servers, &self.diagnostics, &doc);
            doc.replace_diagnostics(diagnostics, &[], None);

            if let Some(diff_base) = self.diff_providers.get_diff_base(&path) {
                doc.set_diff_base(diff_base);
            }
            doc.set_version_control_head(self.diff_providers.get_current_head_name(&path));

            let id = self.new_document(doc);
            self.launch_language_servers(id);

            helix_event::dispatch(DocumentDidOpen {
                editor: self,
                doc: id,
            });

            id
        };

        self.switch(id, action);

        Ok(id)
    }

    pub fn close(&mut self, id: ViewId) {
        // Remove selections for the closed view on all documents.
        for doc in self.documents_mut() {
            doc.remove_view(id);
        }
        self.tree.remove(id);
        self._refresh();
    }

    pub fn close_document(&mut self, doc_id: DocumentId, force: bool) -> Result<(), CloseError> {
        let doc = match self.documents.get(&doc_id) {
            Some(doc) => doc,
            None => return Err(CloseError::DoesNotExist),
        };
        if !force && doc.is_modified() {
            return Err(CloseError::BufferModified(doc.display_name().into_owned()));
        }

        // This will also disallow any follow-up writes
        self.saves.remove(&doc_id);

        enum CloseAction {
            Close(ViewId),
            ReplaceDoc(ViewId, DocumentId),
        }

        let actions: Vec<CloseAction> = self
            .tree
            .views_mut()
            .filter_map(|(view, _focus)| {
                view.remove_document(&doc_id);

                if view.doc == doc_id {
                    // something was previously open in the view, switch to previous doc
                    if let Some(prev_doc) = view.docs_access_history.pop() {
                        Some(CloseAction::ReplaceDoc(view.id, prev_doc))
                    } else {
                        // only the document that is being closed was in the view, close it
                        Some(CloseAction::Close(view.id))
                    }
                } else {
                    None
                }
            })
            .collect();

        for action in actions {
            match action {
                CloseAction::Close(view_id) => {
                    self.close(view_id);
                }
                CloseAction::ReplaceDoc(view_id, doc_id) => {
                    self.replace_document_in_view(view_id, doc_id);
                }
            }
        }

        let doc = self.documents.remove(&doc_id).unwrap();

        // If the document we removed was visible in all views, we will have no more views. We don't
        // want to close the editor just for a simple buffer close, so we need to create a new view
        // containing either an existing document, or a brand new document.
        if self.tree.views().next().is_none() {
            let doc_id = self
                .documents
                .iter()
                .map(|(&doc_id, _)| doc_id)
                .next()
                .unwrap_or_else(|| {
                    self.new_document(Document::default(
                        self.config.clone(),
                        self.syn_loader.clone(),
                    ))
                });
            let view = View::new(doc_id, self.config().gutters.clone());
            let view_id = self.tree.insert(view);
            let doc = doc_mut!(self, &doc_id);
            doc.ensure_view_init(view_id);
            doc.mark_as_focused();
        }

        self._refresh();

        helix_event::dispatch(DocumentDidClose { editor: self, doc });

        Ok(())
    }

    pub fn save<P: Into<PathBuf>>(
        &mut self,
        doc_id: DocumentId,
        path: Option<P>,
        force: bool,
    ) -> anyhow::Result<()> {
        // convert a channel of futures to pipe into main queue one by one
        // via stream.then() ? then push into main future

        let path = path.map(|path| path.into());
        let doc = doc_mut!(self, &doc_id);
        let doc_save_future = doc.save(path, force)?;

        // When a file is written to, notify the file event handler.
        // Note: This can be removed once proper file watching is implemented.
        let handler = self.language_servers.file_event_handler.clone();
        let future = async move {
            let res = doc_save_future.await;
            if let Ok(event) = &res {
                handler.file_changed(event.path.clone());
            }
            res
        };

        use futures_util::stream;

        self.saves
            .get(&doc_id)
            .ok_or_else(|| anyhow::format_err!("saves are closed for this document!"))?
            .send(stream::once(Box::pin(future)))
            .map_err(|err| anyhow!("failed to send save event: {}", err))?;

        self.write_count += 1;

        Ok(())
    }

    #[inline]
    pub fn document(&self, id: DocumentId) -> Option<&Document> {
        self.documents.get(&id)
    }

    #[inline]
    pub fn document_mut(&mut self, id: DocumentId) -> Option<&mut Document> {
        self.documents.get_mut(&id)
    }

    #[inline]
    pub fn documents(&self) -> impl Iterator<Item = &Document> {
        self.documents.values()
    }

    #[inline]
    pub fn documents_mut(&mut self) -> impl Iterator<Item = &mut Document> {
        self.documents.values_mut()
    }

    pub fn document_by_path<P: AsRef<Path>>(&self, path: P) -> Option<&Document> {
        self.documents()
            .find(|doc| doc.path().map(|p| p == path.as_ref()).unwrap_or(false))
    }

    pub fn document_by_path_mut<P: AsRef<Path>>(&mut self, path: P) -> Option<&mut Document> {
        self.documents_mut()
            .find(|doc| doc.path().map(|p| p == path.as_ref()).unwrap_or(false))
    }
}
