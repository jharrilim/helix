use helix_event::dispatch;
use helix_core::Selection;

use crate::{
    events::DocumentFocusLost,
    focus::FocusTarget,
    graphics::Rect,
    tree::{self, LeafKind},
    DocumentId, ViewId,
};

use super::Editor;

impl Editor {
    pub fn is_document_view_focused(&self) -> bool {
        self.tree.try_focused_view().is_some()
    }

    pub fn focused_leaf_kind(&self) -> Option<LeafKind> {
        self.tree.focused_kind()
    }

    pub fn focus_target(&self) -> Option<FocusTarget> {
        let kind = self.tree.focused_kind()?;
        Some(FocusTarget::from_kind(self.tree.focus, kind))
    }

    pub fn git_panel_tree_focused(&self) -> bool {
        self.git
            .panel_id
            .is_some_and(|id| self.tree.focus == id)
    }

    pub fn agent_input_focused(&self) -> bool {
        self.agent.panel_id.is_some_and(|id| {
            self.tree.focus == id && self.agent.focus == crate::agent::AgentFocus::Insert
        })
    }

    pub fn terminal_input_focused(&self) -> bool {
        self.terminal.panel_id.is_some_and(|panel_id| {
            self.tree.focus == panel_id
                && self.terminal.focus == crate::terminal::TerminalFocus::Insert
        })
    }

    pub fn agent_panel_focused(&self) -> bool {
        self.agent_input_focused()
    }

    pub fn terminal_panel_focused(&self) -> bool {
        self.terminal_input_focused()
    }

    pub fn resize(&mut self, area: Rect) {
        if self.tree.resize(area) {
            self._refresh();
        };
    }

    pub fn focus(&mut self, view_id: ViewId) {
        let Some(kind) = self.tree.leaf_kind(view_id) else {
            return;
        };
        self.focus_leaf(FocusTarget::from_kind(view_id, kind));
    }

    pub fn focus_document(&mut self, view_id: ViewId) {
        self.focus_leaf(FocusTarget::Document(view_id));
    }

    pub fn focus_leaf(&mut self, target: FocusTarget) {
        let view_id = target.id();
        if self.tree.focus == view_id {
            return;
        }

        let leaving_document = self.is_document_view_focused();
        self.enter_normal_mode();
        if leaving_document {
            if let Some((view, doc)) = try_current!(self) {
                doc.append_changes_to_history(view);
            }
        }

        match target {
            FocusTarget::Document(id) => self.apply_document_focus(id),
            FocusTarget::Agent(id) => {
                self.tree.focus = id;
                self.agent.focus = crate::agent::AgentFocus::Normal;
            }
            FocusTarget::Terminal(id) => {
                self.tree.focus = id;
                self.terminal.focus = crate::terminal::TerminalFocus::Normal;
                if let Some(panel) = self.tree.terminal_panel(id) {
                    self.terminal.active_session = Some(panel.session_id.clone());
                }
            }
            FocusTarget::Git(id) => {
                self.tree.focus = id;
            }
            FocusTarget::Plan(id) => {
                self.tree.focus = id;
            }
        }
    }

    fn apply_document_focus(&mut self, view_id: ViewId) {
        self.ensure_cursor_in_view(view_id);
        for (view, _focused) in self.tree.views_mut() {
            let doc = doc_mut!(self, &view.doc);
            view.sync_changes(doc);
        }

        let prev_id = self.tree.focus;
        let focus_lost = self.tree.try_get(prev_id).map(|view| view.doc);
        self.tree.focus = view_id;
        if let Some(doc) = self.document_mut(self.tree.get(view_id).doc) {
            doc.mark_as_focused();
        }

        if let Some(doc) = focus_lost {
            dispatch(DocumentFocusLost { editor: self, doc });
        }
    }

    pub fn focus_next(&mut self) {
        self.focus(self.tree.next());
    }

    pub fn focus_prev(&mut self) {
        self.focus(self.tree.prev());
    }

    pub fn focus_direction(&mut self, direction: tree::Direction) {
        let current_view = self.tree.focus;
        if let Some(id) = self.tree.find_split_in_direction(current_view, direction) {
            self.focus(id)
        }
    }

    pub fn swap_split_in_direction(&mut self, direction: tree::Direction) {
        self.tree.swap_split_in_direction(direction);
    }

    pub fn transpose_view(&mut self) {
        self.tree.transpose();
    }

    pub fn ensure_cursor_in_view(&mut self, id: ViewId) {
        let Some(view) = self.tree.try_get(id) else {
            return;
        };
        let config = self.config();
        let doc = doc_mut!(self, &view.doc);
        view.ensure_cursor_in_view(doc, config.scrolloff)
    }

    /// Returns the id of a view that this doc contains a selection for,
    /// making sure it is synced with the current changes
    /// if possible or there are no selections returns current_view
    /// otherwise uses an arbitrary view
    pub fn get_synced_view_id(&mut self, id: DocumentId) -> ViewId {
        let current_view = view_mut!(self);
        let doc = self.documents.get_mut(&id).unwrap();
        if doc.selections().contains_key(&current_view.id) {
            // only need to sync current view if this is not the current doc
            if current_view.doc != id {
                current_view.sync_changes(doc);
            }
            current_view.id
        } else if let Some(view_id) = doc.selections().keys().next() {
            let view_id = *view_id;
            let view = self.tree.get_mut(view_id);
            view.sync_changes(doc);
            view_id
        } else {
            doc.ensure_view_init(current_view.id);
            current_view.id
        }
    }

    pub fn set_cwd(&mut self, path: &std::path::Path) -> std::io::Result<()> {
        self.last_cwd = helix_stdx::env::set_current_working_dir(path)?;
        self.clear_doc_relative_paths();
        Ok(())
    }

    pub(crate) fn clear_doc_relative_paths(&mut self) {
        for doc in self.documents_mut() {
            doc.clear_relative_path();
        }
    }

    pub fn get_last_cwd(&mut self) -> Option<&std::path::Path> {
        self.last_cwd.as_deref()
    }

    pub fn jump_forward(&mut self, view_id: ViewId, count: usize) {
        if let Some((doc_id, selection)) = view_mut!(self, view_id).jumps.forward(count).cloned() {
            self.jump_to(view_id, doc_id, selection);
        }
    }

    pub fn jump_backward(&mut self, view_id: ViewId, count: usize) {
        let view = view_mut!(self, view_id);
        if let Some((doc_id, selection)) = view
            .jumps
            .backward(view_id, doc_mut!(self, &view.doc), count)
            .cloned()
        {
            self.jump_to(view_id, doc_id, selection);
        }
    }

    fn jump_to(&mut self, view_id: ViewId, dest_doc_id: DocumentId, mut selection: Selection) {
        let view = view_mut!(self, view_id);
        let old_doc_id = view.doc;
        if old_doc_id != dest_doc_id {
            let new_doc = doc_mut!(self, &dest_doc_id);
            if let Some(transaction) = view.changes_to_sync(new_doc) {
                let text = new_doc.text().slice(..);
                selection = selection.map(transaction.changes()).ensure_invariants(text);
            }
            self.replace_document_in_view(view_id, dest_doc_id);
            dispatch(DocumentFocusLost {
                editor: self,
                doc: old_doc_id,
            });
        }
        let (view, doc) = current!(self);
        doc.set_selection(view_id, selection);
        view.ensure_cursor_in_view_center(doc, self.config.load().scrolloff);
    }
}
