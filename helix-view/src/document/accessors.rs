use arc_swap::ArcSwap;
use helix_core::auto_pairs::AutoPairs;
use helix_core::doc_formatter::TextFormat;
use helix_core::fold::FoldState;
use helix_core::snippets::SnippetRenderCtx;
use helix_core::syntax::{self, config::LanguageConfiguration};
use helix_core::syntax::config::LanguageServerFeature;
use helix_core::{ChangeSet, Rope, Selection, Syntax};
use helix_lsp::{lsp, Client, LanguageServerId};
use helix_vcs::DiffHandle;
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
use url::Url;

use crate::view::ViewPosition;
use crate::{DocumentId, Editor, Theme, View, ViewId};

use super::encoding_io::from_reader;
use super::language::DEFAULT_TAB_WIDTH;
use super::{Document, ViewData, SCRATCH_BUFFER_NAME};

impl Document {
    pub fn id(&self) -> DocumentId {
        self.id
    }

    /// If there are unsaved modifications.
    pub fn is_modified(&self) -> bool {
        let history = self.history.take();
        let current_revision = history.current_revision();
        self.history.set(history);
        log::debug!(
            "id {} modified - last saved: {}, current: {}",
            self.id,
            self.last_saved_revision,
            current_revision
        );
        current_revision != self.last_saved_revision || !self.changes.is_empty()
    }

    /// Save modifications to history, and so [`Self::is_modified`] will return false.
    pub fn reset_modified(&mut self) {
        let history = self.history.take();
        let current_revision = history.current_revision();
        self.history.set(history);
        self.last_saved_revision = current_revision;
    }

    /// Set the document's latest saved revision to the given one.
    pub fn set_last_saved_revision(&mut self, rev: usize, save_time: SystemTime) {
        log::debug!(
            "doc {} revision updated {} -> {}",
            self.id,
            self.last_saved_revision,
            rev
        );
        self.last_saved_revision = rev;
        self.last_saved_time = save_time;
    }

    /// Get the document's latest saved revision.
    pub fn get_last_saved_revision(&mut self) -> usize {
        self.last_saved_revision
    }

    /// Get the current revision number
    pub fn get_current_revision(&mut self) -> usize {
        let history = self.history.take();
        let current_revision = history.current_revision();
        self.history.set(history);
        current_revision
    }

    /// Corresponding language scope name. Usually `source.<lang>`.
    pub fn language_scope(&self) -> Option<&str> {
        self.language
            .as_ref()
            .map(|language| language.scope.as_str())
    }

    /// Language name for the document. Corresponds to the `name` key in
    /// `languages.toml` configuration.
    pub fn language_name(&self) -> Option<&str> {
        self.language
            .as_ref()
            .map(|language| language.language_id.as_str())
    }

    /// Language ID for the document. Either the `language-id`,
    /// or the document language name if no `language-id` has been specified.
    pub fn language_id(&self) -> Option<&str> {
        self.language_config()?
            .language_server_language_id
            .as_deref()
            .or_else(|| self.language_name())
    }

    /// Corresponding [`LanguageConfiguration`].
    pub fn language_config(&self) -> Option<&LanguageConfiguration> {
        self.language.as_deref()
    }

    /// Current document version, incremented at each change.
    pub fn version(&self) -> i32 {
        self.version
    }

    pub fn word_completion_enabled(&self) -> bool {
        self.language_config()
            .and_then(|lang_config| lang_config.word_completion.and_then(|c| c.enable))
            .unwrap_or_else(|| self.config.load().word_completion.enable)
    }

    pub fn path_completion_enabled(&self) -> bool {
        self.language_config()
            .and_then(|lang_config| lang_config.path_completion)
            .unwrap_or_else(|| self.config.load().path_completion)
    }

    /// maintains the order as configured in the language_servers TOML array
    pub fn language_servers(&self) -> impl Iterator<Item = &helix_lsp::Client> {
        self.language_config().into_iter().flat_map(move |config| {
            config.language_servers.iter().filter_map(move |features| {
                let ls = &**self.language_servers.get(&features.name)?;
                if ls.is_initialized() {
                    Some(ls)
                } else {
                    None
                }
            })
        })
    }

    pub fn remove_language_server_by_name(&mut self, name: &str) -> Option<Arc<Client>> {
        self.language_servers.remove(name)
    }

    pub fn language_servers_with_feature(
        &self,
        feature: LanguageServerFeature,
    ) -> impl Iterator<Item = &helix_lsp::Client> {
        self.language_config().into_iter().flat_map(move |config| {
            config.language_servers.iter().filter_map(move |features| {
                let ls = &**self.language_servers.get(&features.name)?;
                if ls.is_initialized()
                    && ls.supports_feature(feature)
                    && features.has_feature(feature)
                {
                    Some(ls)
                } else {
                    None
                }
            })
        })
    }

    pub fn supports_language_server(&self, id: LanguageServerId) -> bool {
        self.language_servers().any(|l| l.id() == id)
    }

    pub fn diff_handle(&self) -> Option<&DiffHandle> {
        self.diff_handle.as_ref()
    }

    /// Intialize/updates the differ for this document with a new base.
    pub fn set_diff_base(&mut self, diff_base: Vec<u8>) {
        if let Ok((diff_base, ..)) = from_reader(&mut diff_base.as_slice(), Some(self.encoding)) {
            if let Some(differ) = &self.diff_handle {
                differ.update_diff_base(diff_base);
                return;
            }
            self.diff_handle = Some(DiffHandle::new(diff_base, self.text.clone()))
        } else {
            self.diff_handle = None;
        }
    }

    pub fn version_control_head(&self) -> Option<Arc<Box<str>>> {
        self.version_control_head.as_ref().map(|a| a.load_full())
    }

    pub fn set_version_control_head(
        &mut self,
        version_control_head: Option<Arc<ArcSwap<Box<str>>>>,
    ) {
        self.version_control_head = version_control_head;
    }

    #[inline]
    /// Tree-sitter AST tree
    pub fn syntax(&self) -> Option<&Syntax> {
        self.syntax.as_ref()
    }

    /// The width that the tab character is rendered at
    pub fn tab_width(&self) -> usize {
        self.editor_config
            .tab_width
            .map(|n| n.get() as usize)
            .unwrap_or_else(|| {
                self.language_config()
                    .and_then(|config| config.indent.as_ref())
                    .map_or(DEFAULT_TAB_WIDTH, |config| config.tab_width)
            })
    }

    // The width (in spaces) of a level of indentation.
    pub fn indent_width(&self) -> usize {
        self.indent_style.indent_width(self.tab_width())
    }

    /// Whether the document should have a trailing line ending appended on save.
    pub fn insert_final_newline(&self) -> bool {
        self.editor_config
            .insert_final_newline
            .unwrap_or_else(|| self.config.load().insert_final_newline)
    }

    /// Whether the document should trim whitespace preceding line endings on save.
    pub fn trim_trailing_whitespace(&self) -> bool {
        self.editor_config
            .trim_trailing_whitespace
            .unwrap_or_else(|| self.config.load().trim_trailing_whitespace)
    }

    pub fn changes(&self) -> &ChangeSet {
        &self.changes
    }

    #[inline]
    /// File path on disk.
    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }

    pub fn review_comments(&self) -> &[helix_review::ReviewComment] {
        &self.review_comments
    }

    pub fn has_review_comments(&self) -> bool {
        !self.review_comments.is_empty()
    }

    /// File path as a URL.
    pub fn url(&self) -> Option<Url> {
        Url::from_file_path(self.path()?).ok()
    }

    pub fn uri(&self) -> Option<helix_core::Uri> {
        Some(self.path()?.clone().into())
    }

    #[inline]
    pub fn text(&self) -> &Rope {
        &self.text
    }

    #[inline]
    pub fn selection(&self, view_id: ViewId) -> &Selection {
        &self.selections[&view_id]
    }

    #[inline]
    pub fn selections(&self) -> &HashMap<ViewId, Selection> {
        &self.selections
    }

    fn view_data(&self, view_id: ViewId) -> &ViewData {
        self.view_data
            .get(&view_id)
            .expect("This should only be called after ensure_view_init")
    }

    pub(crate) fn view_data_mut(&mut self, view_id: ViewId) -> &mut ViewData {
        self.view_data.entry(view_id).or_default()
    }

    pub(crate) fn get_view_offset(&self, view_id: ViewId) -> Option<ViewPosition> {
        Some(self.view_data.get(&view_id)?.view_position())
    }

    pub fn view_offset(&self, view_id: ViewId) -> ViewPosition {
        self.view_data(view_id).view_position()
    }

    pub fn set_view_offset(&mut self, view_id: ViewId, new_offset: ViewPosition) {
        *self.view_data_mut(view_id).view_position_mut() = new_offset;
    }

    pub fn folds(&self, view_id: ViewId) -> &FoldState {
        &self.view_data(view_id).folds
    }

    pub fn folds_mut(&mut self, view_id: ViewId) -> &mut FoldState {
        &mut self.view_data_mut(view_id).folds
    }

    pub fn relative_path(&self) -> Option<&Path> {
        self.relative_path
            .get_or_init(|| {
                self.path
                    .as_ref()
                    .map(|path| helix_stdx::path::get_relative_path(path).to_path_buf())
            })
            .as_deref()
    }

    pub(crate) fn clear_relative_path(&mut self) {
        self.relative_path.take();
    }

    pub fn display_name(&self) -> Cow<'_, str> {
        self.relative_path()
            .map_or_else(|| SCRATCH_BUFFER_NAME.into(), |path| path.to_string_lossy())
    }

    // transact(Fn) ?

    // -- LSP methods

    #[inline]
    pub fn identifier(&self) -> lsp::TextDocumentIdentifier {
        lsp::TextDocumentIdentifier::new(self.url().unwrap())
    }

    pub fn versioned_identifier(&self) -> lsp::VersionedTextDocumentIdentifier {
        lsp::VersionedTextDocumentIdentifier::new(self.url().unwrap(), self.version)
    }

    pub fn position(
        &self,
        view_id: ViewId,
        offset_encoding: helix_lsp::OffsetEncoding,
    ) -> lsp::Position {
        let text = self.text();

        helix_lsp::util::pos_to_lsp_pos(
            text,
            self.selection(view_id).primary().cursor(text.slice(..)),
            offset_encoding,
        )
    }

    pub fn auto_pairs<'a>(
        &'a self,
        editor: &'a Editor,
        loader: &'a syntax::Loader,
        view: &View,
    ) -> Option<&'a AutoPairs> {
        let global_config = (editor.auto_pairs).as_ref();

        // NOTE: If the user specifies the global auto pairs config as false, then
        //       we want to disable it globally regardless of language settings
        #[allow(clippy::question_mark)]
        {
            if global_config.is_none() {
                return None;
            }
        }

        self.syntax
            .as_ref()
            .and_then(|syntax| {
                let selection = self.selection(view.id).primary();
                let (start, end) = selection.into_byte_range(self.text().slice(..));
                let layer = syntax.layer_for_byte_range(start as u32, end as u32);

                let lang_config = loader.language(syntax.layer(layer).language).config();
                lang_config.auto_pairs.as_ref()
            })
            .or(global_config)
    }

    pub fn snippet_ctx(&self) -> SnippetRenderCtx {
        SnippetRenderCtx {
            // TODO snippet variable resolution
            resolve_var: Box::new(|_| None),
            tab_width: self.tab_width(),
            indent_style: self.indent_style,
            line_ending: self.line_ending.as_str(),
        }
    }

    pub fn text_width(&self) -> usize {
        self.editor_config
            .max_line_length
            .map(|n| n.get() as usize)
            .or_else(|| self.language_config().and_then(|config| config.text_width))
            .unwrap_or_else(|| self.config.load().text_width)
    }

    pub fn text_format(&self, mut viewport_width: u16, theme: Option<&Theme>) -> TextFormat {
        let config = self.config.load();
        let text_width = self.text_width();
        let mut soft_wrap_at_text_width = self
            .language_config()
            .and_then(|config| {
                config
                    .soft_wrap
                    .as_ref()
                    .and_then(|soft_wrap| soft_wrap.wrap_at_text_width)
            })
            .or(config.soft_wrap.wrap_at_text_width)
            .unwrap_or(false);
        if soft_wrap_at_text_width {
            // if the viewport is smaller than the specified
            // width then this setting has no effcet
            if text_width >= viewport_width as usize {
                soft_wrap_at_text_width = false;
            } else {
                viewport_width = text_width as u16;
            }
        }
        let config = self.config.load();
        let editor_soft_wrap = &config.soft_wrap;
        let language_soft_wrap = self
            .language
            .as_ref()
            .and_then(|config| config.soft_wrap.as_ref());
        let enable_soft_wrap = language_soft_wrap
            .and_then(|soft_wrap| soft_wrap.enable)
            .or(editor_soft_wrap.enable)
            .unwrap_or(false);
        let max_wrap = language_soft_wrap
            .and_then(|soft_wrap| soft_wrap.max_wrap)
            .or(config.soft_wrap.max_wrap)
            .unwrap_or(20);
        let max_indent_retain = language_soft_wrap
            .and_then(|soft_wrap| soft_wrap.max_indent_retain)
            .or(editor_soft_wrap.max_indent_retain)
            .unwrap_or(40);
        let wrap_indicator = language_soft_wrap
            .and_then(|soft_wrap| soft_wrap.wrap_indicator.clone())
            .or_else(|| config.soft_wrap.wrap_indicator.clone())
            .unwrap_or_else(|| "↪ ".into());
        let tab_width = self.tab_width() as u16;
        TextFormat {
            soft_wrap: enable_soft_wrap && viewport_width > 10,
            tab_width,
            max_wrap: max_wrap.min(viewport_width / 4),
            max_indent_retain: max_indent_retain.min(viewport_width * 2 / 5),
            // avoid spinning forever when the window manager
            // sets the size to something tiny
            viewport_width,
            wrap_indicator: wrap_indicator.into_boxed_str(),
            wrap_indicator_highlight: theme
                .and_then(|theme| theme.find_highlight("ui.virtual.wrap")),
            soft_wrap_at_text_width,
        }
    }
}
