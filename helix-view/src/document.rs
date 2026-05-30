use arc_swap::access::DynAccess;
use arc_swap::ArcSwap;
use futures_util::future::BoxFuture;
use helix_core::editor_config::EditorConfig;
use helix_core::encoding;
use helix_core::history::{History, State};
use helix_core::indent::IndentStyle;
use helix_core::snippets::ActiveSnippet;
use helix_core::syntax::{self, config::LanguageConfiguration};
use helix_core::text_annotations::Overlay;
use helix_core::{ChangeSet, Diagnostic, LineEnding, Rope, Selection, Syntax, Transaction};
use helix_event::TaskController;
use helix_lsp::{Client, LanguageServerId, LanguageServerName};
use helix_vcs::DiffHandle;
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use std::cell::Cell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Weak};
use std::time::SystemTime;

use crate::editor::Config;
use crate::{DocumentId, ViewId};

mod accessors;
mod annotations;
mod diagnostics;
mod edit;
mod format;
mod encoding_io;
mod language;
mod lifecycle;
mod mode;
mod save;
mod selection;
mod view_data;

#[cfg(test)]
mod tests;

pub use annotations::{
    DocumentColorSwatches, DocumentHighlights, DocumentInlayHints, DocumentInlayHintsId,
    DocumentLink,
};
pub use format::FormatterError;
pub use encoding_io::{from_reader, read_to_string, to_writer};
pub use mode::Mode;
pub use view_data::ViewData;

pub const DEFAULT_LANGUAGE_NAME: &str = "text";

pub const SCRATCH_BUFFER_NAME: &str = "[scratch]";

/// A snapshot of the text of a document that we want to write out to disk
#[derive(Debug, Clone)]
pub struct DocumentSavedEvent {
    pub revision: usize,
    pub save_time: SystemTime,
    pub doc_id: DocumentId,
    pub path: PathBuf,
    pub text: Rope,
}

pub type DocumentSavedEventResult = Result<DocumentSavedEvent, anyhow::Error>;
pub type DocumentSavedEventFuture = BoxFuture<'static, DocumentSavedEventResult>;

#[derive(Debug)]
pub struct SavePoint {
    /// The view this savepoint is associated with
    pub view: ViewId,
    revert: Mutex<Transaction>,
}

#[derive(Debug, thiserror::Error)]
pub enum DocumentOpenError {
    #[error("path must be a regular file, symlink, or directory")]
    IrregularFile,
    #[error(transparent)]
    IoError(#[from] std::io::Error),
}

pub struct Document {
    pub(crate) id: DocumentId,
    text: Rope,
    selections: HashMap<ViewId, Selection>,
    view_data: HashMap<ViewId, ViewData>,
    pub active_snippet: Option<ActiveSnippet>,

    /// Inlay hints annotations for the document, by view.
    ///
    /// To know if they're up-to-date, check the `id` field in `DocumentInlayHints`.
    pub(crate) inlay_hints: HashMap<ViewId, DocumentInlayHints>,
    /// Jump label overlays for each view.
    pub(crate) jump_labels: HashMap<ViewId, Vec<Overlay>>,
    /// LSP document highlights for each view, stored as char ranges.
    pub(crate) document_highlights: HashMap<ViewId, DocumentHighlights>,
    /// Set to `true` when the document is updated, reset to `false` on the next inlay hints
    /// update from the LSP
    pub inlay_hints_oudated: bool,

    path: Option<PathBuf>,
    relative_path: OnceCell<Option<PathBuf>>,
    encoding: &'static encoding::Encoding,
    has_bom: bool,

    pub restore_cursor: bool,

    /// Current indent style.
    pub indent_style: IndentStyle,
    editor_config: EditorConfig,

    /// The document's default line ending.
    pub line_ending: LineEnding,

    pub syntax: Option<Syntax>,
    /// Corresponding language scope name. Usually `source.<lang>`.
    pub language: Option<Arc<LanguageConfiguration>>,

    /// Pending changes since last history commit.
    changes: ChangeSet,
    /// State at last commit. Used for calculating reverts.
    old_state: Option<State>,
    /// Undo tree.
    // It can be used as a cell where we will take it out to get some parts of the history and put
    // it back as it separated from the edits. We could split out the parts manually but that will
    // be more troublesome.
    pub history: Cell<History>,
    pub config: Arc<dyn DynAccess<Config>>,

    savepoints: Vec<Weak<SavePoint>>,

    // Last time we wrote to the file. This will carry the time the file was last opened if there
    // were no saves.
    last_saved_time: SystemTime,

    last_saved_revision: usize,
    version: i32, // should be usize?
    pub(crate) modified_since_accessed: bool,

    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) language_servers: HashMap<LanguageServerName, Arc<Client>>,

    diff_handle: Option<DiffHandle>,
    version_control_head: Option<Arc<ArcSwap<Box<str>>>>,

    // when document was used for most-recent-used buffer picker
    pub focused_at: std::time::Instant,

    pub readonly: bool,

    pub previous_diagnostic_ids: HashMap<LanguageServerId, String>,

    /// Annotations for LSP document color swatches
    pub color_swatches: Option<DocumentColorSwatches>,
    /// Cached LSP document links for navigation (e.g. goto_file).
    pub document_links: Vec<DocumentLink>,
    // NOTE: ideally this would live on the handler for color swatches. This is blocked on a
    // large refactor that would make `&mut Editor` available on the `DocumentDidChange` event.
    pub color_swatch_controller: TaskController,
    /// Per-view task controllers for canceling in-flight document highlight requests.
    pub document_highlight_controllers: HashMap<ViewId, TaskController>,
    pub pull_diagnostic_controller: TaskController,
    pub document_link_controller: TaskController,

    /// Line-anchored review comments displayed below source lines.
    pub(crate) review_comments: Vec<helix_review::ReviewComment>,
    /// When this buffer is a git diff scratch buffer, maps diff lines to source coordinates.
    pub diff_review_source: Option<helix_review::DiffReviewSource>,
    /// Pending text changes to remap session review comments on next sync.
    pub(crate) review_pending_changes: Option<helix_core::ChangeSet>,
    /// Byte cursor in the review comment prompt, mirrored for draft box highlighting.
    pub(crate) review_draft_cursor: Option<usize>,
    /// Focused footer button while drafting a review comment.
    pub(crate) review_draft_button: Option<crate::review::ReviewDraftButton>,

    // NOTE: this field should eventually go away - we should use the Editor's syn_loader instead
    // of storing a copy on every doc. Then we can remove the surrounding `Arc` and use the
    // `ArcSwap` directly.
    syn_loader: Arc<ArcSwap<syntax::Loader>>,
}

use std::fmt;

impl fmt::Debug for Document {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Document")
            .field("id", &self.id)
            .field("text", &self.text)
            .field("selections", &self.selections)
            .field("inlay_hints_oudated", &self.inlay_hints_oudated)
            .field("text_annotations", &self.inlay_hints)
            .field("view_data", &self.view_data)
            .field("path", &self.path)
            .field("encoding", &self.encoding)
            .field("restore_cursor", &self.restore_cursor)
            .field("syntax", &self.syntax)
            .field("language", &self.language)
            .field("changes", &self.changes)
            .field("old_state", &self.old_state)
            .field("last_saved_time", &self.last_saved_time)
            .field("last_saved_revision", &self.last_saved_revision)
            .field("version", &self.version)
            .field("modified_since_accessed", &self.modified_since_accessed)
            .field("diagnostics", &self.diagnostics)
            .finish()
    }
}
