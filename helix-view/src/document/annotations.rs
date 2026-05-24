use helix_core::syntax;
use helix_core::syntax::config::LanguageServerFeature;
use helix_core::text_annotations::{InlineAnnotation, Overlay};
use helix_event::TaskController;
use helix_lsp::{lsp, LanguageServerId};
use std::fmt;

use crate::ViewId;

use super::Document;

#[derive(Debug, Clone, Default)]
pub struct DocumentColorSwatches {
    pub color_swatches: Vec<InlineAnnotation>,
    pub colors: Vec<syntax::Highlight>,
    pub color_swatches_padding: Vec<InlineAnnotation>,
}

/// Highlight ranges returned by LSP `textDocument/documentHighlight` for a view.
#[derive(Debug, Clone, Default)]
pub struct DocumentHighlights {
    pub ranges: Vec<std::ops::Range<usize>>,
}

#[derive(Debug, Clone)]
pub struct DocumentLink {
    /// Character offsets in the document for the link range.
    pub start: usize,
    pub end: usize,
    pub link: lsp::DocumentLink,
    pub language_server_id: LanguageServerId,
}

/// Inlay hints for a single `(Document, View)` combo.
///
/// There are `*_inlay_hints` field for each kind of hints an LSP can send since we offer the
/// option to style theme differently in the theme according to the (currently supported) kinds
/// (`type`, `parameter` and the rest).
///
/// Inlay hints are always `InlineAnnotation`s, not overlays or line-ones: LSP may choose to place
/// them anywhere in the text and will sometime offer config options to move them where the user
/// wants them but it shouldn't be Helix who decides that so we use the most precise positioning.
///
/// The padding for inlay hints needs to be stored separately for before and after (the LSP spec
/// uses 'left' and 'right' but not all text is left to right so let's be correct) padding because
/// the 'before' padding must be added to a layer *before* the regular inlay hints and the 'after'
/// padding comes ... after.
#[derive(Debug, Clone)]
pub struct DocumentInlayHints {
    /// Identifier for the inlay hints stored in this structure. To be checked to know if they have
    /// to be recomputed on idle or not.
    pub id: DocumentInlayHintsId,

    /// Inlay hints of `TYPE` kind, if any.
    pub type_inlay_hints: Vec<InlineAnnotation>,

    /// Inlay hints of `PARAMETER` kind, if any.
    pub parameter_inlay_hints: Vec<InlineAnnotation>,

    /// Inlay hints that are neither `TYPE` nor `PARAMETER`.
    ///
    /// LSPs are not required to associate a kind to their inlay hints, for example Rust-Analyzer
    /// currently never does (February 2023) and the LSP spec may add new kinds in the future that
    /// we want to display even if we don't have some special highlighting for them.
    pub other_inlay_hints: Vec<InlineAnnotation>,

    /// Inlay hint padding. When creating the final `TextAnnotations`, the `before` padding must be
    /// added first, then the regular inlay hints, then the `after` padding.
    pub padding_before_inlay_hints: Vec<InlineAnnotation>,
    pub padding_after_inlay_hints: Vec<InlineAnnotation>,
}

impl DocumentInlayHints {
    /// Generate an empty list of inlay hints with the given ID.
    pub fn empty_with_id(id: DocumentInlayHintsId) -> Self {
        Self {
            id,
            type_inlay_hints: Vec::new(),
            parameter_inlay_hints: Vec::new(),
            other_inlay_hints: Vec::new(),
            padding_before_inlay_hints: Vec::new(),
            padding_after_inlay_hints: Vec::new(),
        }
    }
}

/// Associated with a [`Document`] and [`ViewId`], uniquely identifies the state of inlay hints for
/// for that document and view: if this changed since the last save, the inlay hints for the view
/// should be recomputed.
///
/// We can't store the `ViewOffset` instead of the first and last asked-for lines because if
/// softwrapping changes, the `ViewOffset` may not change while the displayed lines will.
#[derive(Copy, Clone, PartialEq, Eq)]
pub struct DocumentInlayHintsId {
    /// First line for which the inlay hints were requested.
    pub first_line: usize,
    /// Last line for which the inlay hints were requested.
    pub last_line: usize,
}

impl fmt::Debug for DocumentInlayHintsId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Much more agreable to read when debugging
        f.debug_struct("DocumentInlayHintsId")
            .field("lines", &(self.first_line..self.last_line))
            .finish()
    }
}

impl Document {
    pub fn set_inlay_hints(&mut self, view_id: ViewId, inlay_hints: DocumentInlayHints) {
        self.inlay_hints.insert(view_id, inlay_hints);
    }

    pub fn set_jump_labels(&mut self, view_id: ViewId, labels: Vec<Overlay>) {
        self.jump_labels.insert(view_id, labels);
    }

    pub fn remove_jump_labels(&mut self, view_id: ViewId) {
        self.jump_labels.remove(&view_id);
    }

    pub fn set_document_highlights(
        &mut self,
        view_id: ViewId,
        ranges: Vec<std::ops::Range<usize>>,
    ) {
        if ranges.is_empty() {
            self.document_highlights.remove(&view_id);
        } else {
            self.document_highlights
                .insert(view_id, DocumentHighlights { ranges });
        }
    }

    pub fn clear_document_highlights(&mut self, view_id: ViewId) {
        self.document_highlights.remove(&view_id);
    }

    pub fn clear_all_document_highlights(&mut self) {
        self.document_highlights.clear();
        self.document_highlight_controllers.clear();
    }

    pub fn document_highlights(&self, view_id: ViewId) -> Option<&[std::ops::Range<usize>]> {
        self.document_highlights
            .get(&view_id)
            .map(|highlights| highlights.ranges.as_slice())
    }

    pub fn document_highlight_controller(&mut self, view_id: ViewId) -> &mut TaskController {
        self.document_highlight_controllers
            .entry(view_id)
            .or_default()
    }

    /// Get the inlay hints for this document and `view_id`.
    pub fn inlay_hints(&self, view_id: ViewId) -> Option<&DocumentInlayHints> {
        self.inlay_hints.get(&view_id)
    }

    /// Completely removes all the inlay hints saved for the document, dropping them to free memory
    /// (since it often means inlay hints have been fully deactivated).
    pub fn reset_all_inlay_hints(&mut self) {
        self.inlay_hints = Default::default();
    }

    pub fn has_language_server_with_feature(&self, feature: LanguageServerFeature) -> bool {
        self.language_servers_with_feature(feature).next().is_some()
    }
}
