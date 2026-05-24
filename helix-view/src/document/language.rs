use anyhow::{anyhow, Error};
use helix_core::editor_config::EditorConfig;
use helix_core::encoding::Encoding;
use helix_core::indent::{auto_detect_indent_style, IndentStyle};
use helix_core::line_ending::auto_detect_line_ending;
use helix_core::syntax::{self, Syntax};
use helix_stdx::faccess::readonly;
use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;

use super::Document;

pub(crate) const DEFAULT_INDENT: IndentStyle = IndentStyle::Tabs;
pub(crate) const DEFAULT_TAB_WIDTH: usize = 4;

impl Document {
    pub fn detect_language(&mut self, loader: &syntax::Loader) {
        self.set_language(self.detect_language_config(loader), loader);
    }

    /// Detect the programming language based on the file type.
    pub fn detect_language_config(
        &self,
        loader: &syntax::Loader,
    ) -> Option<Arc<syntax::config::LanguageConfiguration>> {
        let language = loader
            .language_for_filename(self.path.as_ref()?)
            .or_else(|| loader.language_for_shebang(self.text().slice(..)))?;

        Some(loader.language(language).config().clone())
    }

    /// Detect the indentation used in the file, or otherwise defaults to the language indentation
    /// configured in `languages.toml`, with a fallback to tabs if it isn't specified. Line ending
    /// is likewise auto-detected, and will remain unchanged if no line endings were detected.
    pub fn detect_indent_and_line_ending(&mut self) {
        self.indent_style = if let Some(indent_style) = self.editor_config.indent_style {
            indent_style
        } else {
            auto_detect_indent_style(&self.text).unwrap_or_else(|| {
                self.language_config()
                    .and_then(|config| config.indent.as_ref())
                    .map_or(DEFAULT_INDENT, |config| IndentStyle::from_str(&config.unit))
            })
        };
        if let Some(line_ending) = self
            .editor_config
            .line_ending
            .or_else(|| auto_detect_line_ending(&self.text))
        {
            self.line_ending = line_ending;
        }
    }

    pub fn detect_editor_config(&mut self) {
        if self.config.load().editor_config {
            if let Some(path) = self.path.as_ref() {
                self.editor_config = EditorConfig::find(path);
            }
        }
    }

    pub fn pickup_last_saved_time(&mut self) {
        self.last_saved_time = match self.path() {
            Some(path) => match path.metadata() {
                Ok(metadata) => match metadata.modified() {
                    Ok(mtime) => mtime,
                    Err(err) => {
                        log::debug!("Could not fetch file system's mtime, falling back to current system time: {}", err);
                        SystemTime::now()
                    }
                },
                Err(err) => {
                    log::debug!("Could not fetch file system's mtime, falling back to current system time: {}", err);
                    SystemTime::now()
                }
            },
            None => SystemTime::now(),
        };
    }

    // Detect if the file is readonly and change the readonly field if necessary (unix only)
    pub fn detect_readonly(&mut self) {
        // Allows setting the flag for files the user cannot modify, like root files
        self.readonly = match &self.path {
            None => false,
            Some(p) => readonly(p),
        };
    }

    /// Sets the [`Document`]'s encoding with the encoding correspondent to `label`.
    pub fn set_encoding(&mut self, label: &str) -> Result<(), Error> {
        let encoding =
            Encoding::for_label(label.as_bytes()).ok_or_else(|| anyhow!("unknown encoding"))?;

        self.encoding = encoding;

        Ok(())
    }

    /// Returns the [`Document`]'s current encoding.
    pub fn encoding(&self) -> &'static Encoding {
        self.encoding
    }

    /// sets the document path without sending events to various
    /// observers (like LSP), in most cases `Editor::set_doc_path`
    /// should be used instead
    pub fn set_path(&mut self, path: Option<&Path>) {
        let path = path.map(helix_stdx::path::canonicalize);

        // `take` to remove any prior relative path that may have existed.
        // This will get set in `relative_path()`.
        self.relative_path.take();

        // if parent doesn't exist we still want to open the document
        // and error out when document is saved
        self.path = path;

        self.detect_readonly();
        self.pickup_last_saved_time();
    }

    /// Set the programming language for the file and load associated data (e.g. highlighting)
    /// if it exists.
    pub fn set_language(
        &mut self,
        language_config: Option<Arc<syntax::config::LanguageConfiguration>>,
        loader: &syntax::Loader,
    ) {
        self.language = language_config;
        self.syntax = self.language.as_ref().and_then(|config| {
            Syntax::new(self.text.slice(..), config.language(), loader)
                .map_err(|err| {
                    // `NoRootConfig` means that there was an issue loading the language/syntax
                    // config for the root language of the document. An error must have already
                    // been logged by `LanguageData::syntax_config`.
                    if err != syntax::HighlighterError::NoRootConfig {
                        log::warn!("Error building syntax for '{}': {err}", self.display_name());
                    }
                })
                .ok()
        });
    }

    /// Set the programming language for the file if you know the language but don't have the
    /// [`syntax::config::LanguageConfiguration`] for it.
    pub fn set_language_by_language_id(
        &mut self,
        language_id: &str,
        loader: &syntax::Loader,
    ) -> anyhow::Result<()> {
        let language = loader
            .language_for_name(language_id)
            .ok_or_else(|| anyhow!("invalid language id: {}", language_id))?;
        let config = loader.language(language).config().clone();
        self.set_language(Some(config), loader);
        Ok(())
    }

}
