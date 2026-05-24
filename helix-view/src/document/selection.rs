use helix_core::{Range, Selection};

use crate::events::SelectionDidChange;
use crate::ViewId;

use super::Document;

impl Document {
    pub fn set_selection(&mut self, view_id: ViewId, selection: Selection) {
        // TODO: use a transaction?
        self.selections
            .insert(view_id, selection.ensure_invariants(self.text().slice(..)));
        helix_event::dispatch(SelectionDidChange {
            doc: self,
            view: view_id,
        })
    }

    /// Find the origin selection of the text in a document, i.e. where
    /// a single cursor would go if it were on the first grapheme. If
    /// the text is empty, returns (0, 0).
    pub fn origin(&self) -> Range {
        if self.text().len_chars() == 0 {
            return Range::new(0, 0);
        }

        Range::new(0, 1).grapheme_aligned(self.text().slice(..))
    }

    /// Reset the view's selection on this document to the
    /// [origin](Document::origin) cursor.
    pub fn reset_selection(&mut self, view_id: ViewId) {
        let origin = self.origin();
        self.set_selection(view_id, Selection::single(origin.anchor, origin.head));
    }

    /// Initializes a new selection and view_data for the given view
    /// if it does not already have them.
    pub fn ensure_view_init(&mut self, view_id: ViewId) {
        if !self.selections.contains_key(&view_id) {
            self.reset_selection(view_id);
        }

        self.view_data_mut(view_id);
    }

    /// Mark document as recent used for MRU sorting
    pub fn mark_as_focused(&mut self) {
        self.focused_at = std::time::Instant::now();
    }

    /// Remove a view's selection and inlay hints from this document.
    pub fn remove_view(&mut self, view_id: ViewId) {
        self.selections.remove(&view_id);
        self.inlay_hints.remove(&view_id);
        self.jump_labels.remove(&view_id);
        self.document_highlights.remove(&view_id);
        self.document_highlight_controllers.remove(&view_id);
    }

}
