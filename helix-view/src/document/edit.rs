use helix_core::history::{State, UndoKind};
use helix_core::text_annotations::InlineAnnotation;
use helix_core::{ChangeSet, Transaction};
use parking_lot::Mutex;
use std::mem;
use std::sync::Arc;

use crate::events::{DocumentDidChange, SelectionDidChange};
use crate::{View, ViewId};

use super::{Document, DocumentInlayHints, SavePoint};

fn take_with<T, F>(mut_ref: &mut T, f: F)
where
    T: Default,
    F: FnOnce(T) -> T,
{
    *mut_ref = f(mem::take(mut_ref));
}

impl Document {
    fn apply_impl(
        &mut self,
        transaction: &Transaction,
        view_id: ViewId,
        emit_lsp_notification: bool,
    ) -> bool {
        use helix_core::Assoc;

        let old_doc = self.text().clone();
        let changes = transaction.changes();
        if !changes.apply(&mut self.text) {
            return false;
        }

        if changes.is_empty() {
            if let Some(selection) = transaction.selection() {
                self.selections.insert(
                    view_id,
                    selection.clone().ensure_invariants(self.text.slice(..)),
                );
                helix_event::dispatch(SelectionDidChange {
                    doc: self,
                    view: view_id,
                });
            }
            return true;
        }

        self.modified_since_accessed = true;
        self.version += 1;

        for selection in self.selections.values_mut() {
            *selection = selection
                .clone()
                // Map through changes
                .map(transaction.changes())
                // Ensure all selections across all views still adhere to invariants.
                .ensure_invariants(self.text.slice(..));
        }

        for view_data in self.view_data.values_mut() {
            view_data.view_position_mut().anchor = transaction
                .changes()
                .map_pos(view_data.view_position().anchor, Assoc::Before);
        }

        if let Some((start_line, end_line)) =
            helix_core::fold::changed_line_range(old_doc.slice(..), changes)
        {
            for view_data in self.view_data.values_mut() {
                view_data.folds.invalidate_lines(start_line, end_line);
            }
        }

        // generate revert to savepoint
        if !self.savepoints.is_empty() {
            let revert = transaction.invert(&old_doc);
            self.savepoints
                .retain_mut(|save_point| match save_point.upgrade() {
                    Some(savepoint) => {
                        let mut revert_to_savepoint = savepoint.revert.lock();
                        *revert_to_savepoint =
                            revert.clone().compose(mem::take(&mut revert_to_savepoint));
                        true
                    }
                    None => false,
                })
        }

        // update tree-sitter syntax tree
        if let Some(syntax) = &mut self.syntax {
            let loader = self.syn_loader.load();
            if let Err(err) = syntax.update(
                old_doc.slice(..),
                self.text.slice(..),
                transaction.changes(),
                &loader,
            ) {
                log::error!("TS parser failed, disabling TS for the current buffer: {err}");
                self.syntax = None;
            }
        }

        // TODO: all of that should likely just be hooks
        // start computing the diff in parallel
        if let Some(diff_handle) = &self.diff_handle {
            diff_handle.update_document(self.text.clone(), false);
        }

        // map diagnostics over changes too
        changes.update_positions(self.diagnostics.iter_mut().map(|diagnostic| {
            let assoc = if diagnostic.starts_at_word {
                Assoc::BeforeWord
            } else {
                Assoc::After
            };
            (&mut diagnostic.range.start, assoc)
        }));
        changes.update_positions(self.diagnostics.iter_mut().filter_map(|diagnostic| {
            if diagnostic.zero_width {
                // for zero width diagnostics treat the diagnostic as a point
                // rather than a range
                return None;
            }
            let assoc = if diagnostic.ends_at_word {
                Assoc::AfterWord
            } else {
                Assoc::Before
            };
            Some((&mut diagnostic.range.end, assoc))
        }));
        self.diagnostics.retain_mut(|diagnostic| {
            if diagnostic.zero_width {
                diagnostic.range.end = diagnostic.range.start
            } else if diagnostic.range.start >= diagnostic.range.end {
                return false;
            }
            diagnostic.line = self.text.char_to_line(diagnostic.range.start);
            true
        });

        self.diagnostics.sort_by_key(|diagnostic| {
            (
                diagnostic.range,
                diagnostic.severity,
                diagnostic.provider.clone(),
            )
        });

        // Update the inlay hint annotations' positions, helping ensure they are displayed in the proper place
        let apply_inlay_hint_changes = |annotations: &mut Vec<InlineAnnotation>| {
            changes.update_positions(
                annotations
                    .iter_mut()
                    .map(|annotation| (&mut annotation.char_idx, Assoc::After)),
            );
        };

        self.inlay_hints_oudated = true;
        for text_annotation in self.inlay_hints.values_mut() {
            let DocumentInlayHints {
                id: _,
                type_inlay_hints,
                parameter_inlay_hints,
                other_inlay_hints,
                padding_before_inlay_hints,
                padding_after_inlay_hints,
            } = text_annotation;

            apply_inlay_hint_changes(padding_before_inlay_hints);
            apply_inlay_hint_changes(type_inlay_hints);
            apply_inlay_hint_changes(parameter_inlay_hints);
            apply_inlay_hint_changes(other_inlay_hints);
            apply_inlay_hint_changes(padding_after_inlay_hints);
        }

        for highlights in self.document_highlights.values_mut() {
            let text_len = self.text.len_chars();
            let mut updated = Vec::with_capacity(highlights.ranges.len());
            for mut range in highlights.ranges.drain(..) {
                changes.update_positions(
                    [
                        (&mut range.start, Assoc::After),
                        (&mut range.end, Assoc::After),
                    ]
                    .into_iter(),
                );
                if range.start >= text_len {
                    continue;
                }
                let end = range.end.min(text_len);
                if range.start < end {
                    updated.push(range.start..end);
                }
            }
            highlights.ranges = updated;
        }

        helix_event::dispatch(DocumentDidChange {
            doc: self,
            view: view_id,
            old_text: &old_doc,
            changes,
            ghost_transaction: !emit_lsp_notification,
        });

        // if specified, the current selection should instead be replaced by transaction.selection
        if let Some(selection) = transaction.selection() {
            self.selections.insert(
                view_id,
                selection.clone().ensure_invariants(self.text.slice(..)),
            );
            helix_event::dispatch(SelectionDidChange {
                doc: self,
                view: view_id,
            });
        }

        true
    }

    fn apply_inner(
        &mut self,
        transaction: &Transaction,
        view_id: ViewId,
        emit_lsp_notification: bool,
    ) -> bool {
        // store the state just before any changes are made. This allows us to undo to the
        // state just before a transaction was applied.
        if self.changes.is_empty() && !transaction.changes().is_empty() {
            self.old_state = Some(State {
                doc: self.text.clone(),
                selection: self.selection(view_id).clone(),
            });
        }

        let success = self.apply_impl(transaction, view_id, emit_lsp_notification);

        if !transaction.changes().is_empty() {
            // Compose this transaction with the previous one
            take_with(&mut self.changes, |changes| {
                changes.compose(transaction.changes().clone())
            });
        }
        success
    }
    /// Apply a [`Transaction`] to the [`Document`] to change its text.
    pub fn apply(&mut self, transaction: &Transaction, view_id: ViewId) -> bool {
        self.apply_inner(transaction, view_id, true)
    }

    /// Apply a [`Transaction`] to the [`Document`] to change its text
    /// without notifying the language servers. This is useful for temporary transactions
    /// that must not influence the server.
    pub fn apply_temporary(&mut self, transaction: &Transaction, view_id: ViewId) -> bool {
        self.apply_inner(transaction, view_id, false)
    }

    fn undo_redo_impl(&mut self, view: &mut View, undo: bool) -> bool {
        if undo {
            self.append_changes_to_history(view);
        } else if !self.changes.is_empty() {
            return false;
        }
        let mut history = self.history.take();
        let txn = if undo { history.undo() } else { history.redo() };
        let success = if let Some(txn) = txn {
            self.apply_impl(txn, view.id, true)
        } else {
            false
        };
        self.history.set(history);

        if success {
            // reset changeset to fix len
            self.changes = ChangeSet::new(self.text().slice(..));
            // Sync with changes with the jumplist selections.
            view.sync_changes(self);
        }
        success
    }

    /// Undo the last modification to the [`Document`]. Returns whether the undo was successful.
    pub fn undo(&mut self, view: &mut View) -> bool {
        self.undo_redo_impl(view, true)
    }

    /// Redo the last modification to the [`Document`]. Returns whether the redo was successful.
    pub fn redo(&mut self, view: &mut View) -> bool {
        self.undo_redo_impl(view, false)
    }

    /// Creates a reference counted snapshot (called savpepoint) of the document.
    ///
    /// The snapshot will remain valid (and updated) idenfinitly as long as ereferences to it exist.
    /// Restoring the snapshot will restore the selection and the contents of the document to
    /// the state it had when this function was called.
    pub fn savepoint(&mut self, view: &View) -> Arc<SavePoint> {
        let revert = Transaction::new(self.text()).with_selection(self.selection(view.id).clone());
        // check if there is already an existing (identical) savepoint around
        if let Some(savepoint) = self
            .savepoints
            .iter()
            .rev()
            .find_map(|savepoint| savepoint.upgrade())
        {
            let transaction = savepoint.revert.lock();
            if savepoint.view == view.id
                && transaction.changes().is_empty()
                && transaction.selection() == revert.selection()
            {
                drop(transaction);
                return savepoint;
            }
        }
        let savepoint = Arc::new(SavePoint {
            view: view.id,
            revert: Mutex::new(revert),
        });
        self.savepoints.push(Arc::downgrade(&savepoint));
        savepoint
    }

    pub fn restore(&mut self, view: &mut View, savepoint: &SavePoint, emit_lsp_notification: bool) {
        assert_eq!(
            savepoint.view, view.id,
            "Savepoint must not be used with a different view!"
        );
        // search and remove savepoint using a ptr comparison
        // this avoids a deadlock as we need to lock the mutex
        let savepoint_idx = self
            .savepoints
            .iter()
            .position(|savepoint_ref| std::ptr::eq(savepoint_ref.as_ptr(), savepoint))
            .expect("Savepoint must belong to this document");

        let savepoint_ref = self.savepoints.remove(savepoint_idx);
        let mut revert = savepoint.revert.lock();
        self.apply_inner(&revert, view.id, emit_lsp_notification);
        *revert = Transaction::new(self.text()).with_selection(self.selection(view.id).clone());
        self.savepoints.push(savepoint_ref)
    }

    fn earlier_later_impl(&mut self, view: &mut View, uk: UndoKind, earlier: bool) -> bool {
        if earlier {
            self.append_changes_to_history(view);
        } else if !self.changes.is_empty() {
            return false;
        }
        let txns = if earlier {
            self.history.get_mut().earlier(uk)
        } else {
            self.history.get_mut().later(uk)
        };
        let mut success = false;
        for txn in txns {
            if self.apply_impl(&txn, view.id, true) {
                success = true;
            }
        }
        if success {
            // reset changeset to fix len
            self.changes = ChangeSet::new(self.text().slice(..));
            // Sync with changes with the jumplist selections.
            view.sync_changes(self);
        }
        success
    }

    /// Undo modifications to the [`Document`] according to `uk`.
    pub fn earlier(&mut self, view: &mut View, uk: UndoKind) -> bool {
        self.earlier_later_impl(view, uk, true)
    }

    /// Redo modifications to the [`Document`] according to `uk`.
    pub fn later(&mut self, view: &mut View, uk: UndoKind) -> bool {
        self.earlier_later_impl(view, uk, false)
    }

    /// Commit pending changes to history
    pub fn append_changes_to_history(&mut self, view: &mut View) {
        if self.changes.is_empty() {
            return;
        }

        let new_changeset = ChangeSet::new(self.text().slice(..));
        let changes = std::mem::replace(&mut self.changes, new_changeset);
        // Instead of doing this messy merge we could always commit, and based on transaction
        // annotations either add a new layer or compose into the previous one.
        let transaction =
            Transaction::from(changes).with_selection(self.selection(view.id).clone());

        // HAXX: we need to reconstruct the state as it was before the changes..
        let old_state = self.old_state.take().expect("no old_state available");

        let mut history = self.history.take();
        history.commit_revision(&transaction, &old_state);
        self.history.set(history);

        // Update jumplist entries in the view.
        view.apply(&transaction, self);
    }
}
