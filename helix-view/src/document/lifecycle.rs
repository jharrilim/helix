use anyhow::{bail, Error};
use arc_swap::access::DynAccess;
use arc_swap::ArcSwap;
use helix_core::editor_config::EditorConfig;
use helix_core::encoding;
use helix_core::encoding::Encoding;
use helix_core::history::History;
use helix_core::syntax;
use helix_core::{ChangeSet, LineEnding, Rope};
use helix_event::TaskController;
use helix_vcs::DiffProviderRegistry;
use once_cell::sync::OnceCell;
use std::cell::Cell;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;

use crate::editor::Config;
use crate::{DocumentId, View};

use super::language::DEFAULT_INDENT;
use super::{Document, DocumentOpenError};
use super::encoding_io::from_reader;

impl Document {
    pub fn from(
        text: Rope,
        encoding_with_bom_info: Option<(&'static Encoding, bool)>,
        config: Arc<dyn DynAccess<Config>>,
        syn_loader: Arc<ArcSwap<syntax::Loader>>,
    ) -> Self {
        let (encoding, has_bom) = encoding_with_bom_info.unwrap_or((encoding::UTF_8, false));
        let line_ending = config.load().default_line_ending.into();
        let changes = ChangeSet::new(text.slice(..));
        let old_state = None;

        Self {
            id: DocumentId::default(),
            active_snippet: None,
            path: None,
            relative_path: OnceCell::new(),
            encoding,
            has_bom,
            text,
            selections: HashMap::default(),
            inlay_hints: HashMap::default(),
            inlay_hints_oudated: false,
            view_data: Default::default(),
            indent_style: DEFAULT_INDENT,
            editor_config: EditorConfig::default(),
            line_ending,
            restore_cursor: false,
            syntax: None,
            language: None,
            changes,
            old_state,
            diagnostics: Vec::new(),
            version: 0,
            history: Cell::new(History::default()),
            savepoints: Vec::new(),
            last_saved_time: SystemTime::now(),
            last_saved_revision: 0,
            modified_since_accessed: false,
            language_servers: HashMap::new(),
            diff_handle: None,
            config,
            version_control_head: None,
            focused_at: std::time::Instant::now(),
            readonly: false,
            jump_labels: HashMap::new(),
            document_highlights: HashMap::new(),
            color_swatches: None,
            document_links: Vec::new(),
            color_swatch_controller: TaskController::new(),
            document_highlight_controllers: HashMap::new(),
            syn_loader,
            previous_diagnostic_ids: HashMap::new(),
            pull_diagnostic_controller: TaskController::new(),
            document_link_controller: TaskController::new(),
            review_comments: Vec::new(),
            diff_review_source: None,
            review_pending_changes: None,
        }
    }

    pub fn default(
        config: Arc<dyn DynAccess<Config>>,
        syn_loader: Arc<ArcSwap<syntax::Loader>>,
    ) -> Self {
        let line_ending: LineEnding = config.load().default_line_ending.into();
        let text = Rope::from(line_ending.as_str());
        Self::from(text, None, config, syn_loader)
    }

    // TODO: async fn?
    /// Create a new document from `path`. Encoding is auto-detected, but it can be manually
    /// overwritten with the `encoding` parameter.
    pub fn open(
        path: &Path,
        mut encoding: Option<&'static Encoding>,
        detect_language: bool,
        config: Arc<dyn DynAccess<Config>>,
        syn_loader: Arc<ArcSwap<syntax::Loader>>,
    ) -> Result<Self, DocumentOpenError> {
        // If the path is not a regular file (e.g.: /dev/random) it should not be opened.
        if path.metadata().is_ok_and(|metadata| !metadata.is_file()) {
            return Err(DocumentOpenError::IrregularFile);
        }

        let editor_config = if config.load().editor_config {
            EditorConfig::find(path)
        } else {
            EditorConfig::default()
        };
        encoding = encoding.or(editor_config.encoding);

        // Open the file if it exists, otherwise assume it is a new file (and thus empty).
        let (rope, encoding, has_bom) = if path.exists() {
            let mut file = std::fs::File::open(path)?;
            from_reader(&mut file, encoding)?
        } else {
            let line_ending = editor_config
                .line_ending
                .unwrap_or_else(|| config.load().default_line_ending.into());
            let encoding = encoding.unwrap_or(encoding::UTF_8);
            (Rope::from(line_ending.as_str()), encoding, false)
        };

        let loader = syn_loader.load();
        let mut doc = Self::from(rope, Some((encoding, has_bom)), config, syn_loader);

        // set the path and try detecting the language
        doc.set_path(Some(path));
        if detect_language {
            doc.detect_language(&loader);
        }

        doc.editor_config = editor_config;
        doc.detect_indent_and_line_ending();

        Ok(doc)
    }

    /// Reload the document from its path.
    pub fn reload(
        &mut self,
        view: &mut View,
        provider_registry: &DiffProviderRegistry,
    ) -> Result<(), Error> {
        let encoding = self.encoding;
        let path = match self.path() {
            None => return Ok(()),
            Some(path) => match path.exists() {
                true => path.to_owned(),
                false => bail!("can't find file to reload from {:?}", self.display_name()),
            },
        };

        // Once we have a valid path we check if its readonly status has changed
        self.detect_readonly();

        let mut file = std::fs::File::open(&path)?;
        let (rope, ..) = from_reader(&mut file, Some(encoding))?;

        // Calculate the difference between the buffer and source text, and apply it.
        // This is not considered a modification of the contents of the file regardless
        // of the encoding.
        let transaction = helix_core::diff::compare_ropes(self.text(), &rope);
        self.apply(&transaction, view.id);
        self.append_changes_to_history(view);
        self.reset_modified();
        self.pickup_last_saved_time();
        self.detect_indent_and_line_ending();

        match provider_registry.get_diff_base(&path) {
            Some(diff_base) => self.set_diff_base(diff_base),
            None => self.diff_handle = None,
        }

        self.version_control_head = provider_registry.get_current_head_name(&path);

        Ok(())
    }
}
