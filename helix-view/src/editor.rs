use crate::{
    document::{DocumentSavedEventFuture, DocumentSavedEventResult, Mode, SavePoint},
    graphics::Rect,
    handlers::Handlers,
    info::Info,
    input::KeyEvent,
    register::Registers,
    theme::{Loader as ThemeLoader, Theme},
    tree::Tree,
    Document, DocumentId, View,
};
use helix_vcs::DiffProviderRegistry;

use futures_util::stream::select_all::SelectAll;
use helix_dap::{self as dap, registry::DebugAdapterId};
use helix_lsp::{Call, lsp as lsp_types, LanguageServerId};

use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap, VecDeque},
    path::PathBuf,
    pin::Pin,
    sync::Arc,
};

use tokio::{
    sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
    time::{sleep, Duration, Instant, Sleep},
};

pub use helix_core::diagnostic::Severity;
use helix_core::{
    auto_pairs::AutoPairs,
    diagnostic::DiagnosticProvider,
    syntax,
    Change, Range, Selection, Uri,
};

use arc_swap::{
    access::{DynAccess, DynGuard},
    ArcSwap,
};

mod config;
mod cursor;
mod diagnostics;
mod documents;
mod events;
mod focus;
mod lsp;
mod mode;
mod panels;
mod theme;

pub use config::*;
pub use cursor::CursorCache;

pub const DIR_STACK_CAP: usize = 10;

#[derive(Debug, Clone, Default)]
pub struct Breakpoint {
    pub id: Option<usize>,
    pub verified: bool,
    pub message: Option<String>,

    pub line: usize,
    pub column: Option<usize>,
    pub condition: Option<String>,
    pub hit_condition: Option<String>,
    pub log_message: Option<String>,
}

use futures_util::stream::{Flatten, Once};
use tokio_stream::wrappers::UnboundedReceiverStream;

pub(crate) type Diagnostics = BTreeMap<Uri, Vec<(lsp_types::Diagnostic, DiagnosticProvider)>>;

pub struct Editor {
    /// Current editing mode.
    pub mode: Mode,
    pub tree: Tree,
    pub next_document_id: DocumentId,
    pub documents: BTreeMap<DocumentId, Document>,

    // We Flatten<> to resolve the inner DocumentSavedEventFuture. For that we need a stream of streams, hence the Once<>.
    // https://stackoverflow.com/a/66875668
    pub saves: HashMap<DocumentId, UnboundedSender<Once<DocumentSavedEventFuture>>>,
    pub save_queue: SelectAll<Flatten<UnboundedReceiverStream<Once<DocumentSavedEventFuture>>>>,
    pub write_count: usize,

    pub count: Option<std::num::NonZeroUsize>,
    pub selected_register: Option<char>,
    pub registers: Registers,
    pub macro_recording: Option<(char, Vec<KeyEvent>)>,
    pub macro_replaying: Vec<char>,
    pub language_servers: helix_lsp::Registry,
    pub diagnostics: Diagnostics,
    pub diff_providers: DiffProviderRegistry,

    pub debug_adapters: dap::registry::Registry,
    pub breakpoints: HashMap<PathBuf, Vec<Breakpoint>>,

    pub syn_loader: Arc<ArcSwap<syntax::Loader>>,
    pub theme_loader: Arc<ThemeLoader>,
    /// last_theme is used for theme previews. We store the current theme here,
    /// and if previewing is cancelled, we can return to it.
    pub last_theme: Option<Theme>,
    /// The currently applied editor theme. While previewing a theme, the previewed theme
    /// is set here.
    pub theme: Theme,

    /// The primary Selection prior to starting a goto_line_number preview. This is
    /// restored when the preview is aborted, or added to the jumplist when it is
    /// confirmed.
    pub last_selection: Option<Selection>,

    pub status_msg: Option<(Cow<'static, str>, Severity)>,
    pub autoinfo: Option<Info>,

    pub config: Arc<dyn DynAccess<Config>>,
    pub auto_pairs: Option<AutoPairs>,

    pub idle_timer: Pin<Box<Sleep>>,
    redraw_timer: Pin<Box<Sleep>>,
    last_motion: Option<Motion>,
    pub last_completion: Option<CompleteAction>,
    pub last_cwd: Option<PathBuf>,
    pub dir_stack: VecDeque<PathBuf>,

    pub exit_code: i32,

    pub config_events: (UnboundedSender<ConfigEvent>, UnboundedReceiver<ConfigEvent>),
    pub needs_redraw: bool,
    /// Cached position of the cursor calculated during rendering.
    /// The content of `cursor_cache` is returned by `Editor::cursor` if
    /// set to `Some(_)`. The value will be cleared after it's used.
    /// If `cursor_cache` is `None` then the `Editor::cursor` function will
    /// calculate the cursor position.
    ///
    /// `Some(None)` represents a cursor position outside of the visible area.
    /// This will just cause `Editor::cursor` to return `None`.
    ///
    /// This cache is only a performance optimization to
    /// avoid calculating the cursor position multiple
    /// times during rendering and should not be set by other functions.
    pub handlers: Handlers,

    pub mouse_down_range: Option<Range>,
    pub cursor_cache: CursorCache,
    pub agent: crate::agent::AgentState,
    pub terminal: crate::terminal::TerminalState,
    pub git: crate::git::GitState,
    pub plan: crate::plan::PlanState,
    pub review: crate::review::ReviewState,
    pub review_panel: crate::review_panel::ReviewPanelState,
}

pub type Motion = Box<dyn Fn(&mut Editor)>;

#[derive(Debug)]
pub enum EditorEvent {
    DocumentSaved(DocumentSavedEventResult),
    ConfigEvent(ConfigEvent),
    LanguageServerMessage((LanguageServerId, Call)),
    DebuggerEvent((DebugAdapterId, dap::Payload)),
    IdleTimer,
    Redraw,
}

#[derive(Debug, Clone)]
pub enum ConfigEvent {
    Refresh,
    Update(Box<Config>),
    ThemeChanged,
    SetTheme(crate::theme::Config),
}

#[derive(Debug, Clone)]
pub enum CompleteAction {
    Triggered,
    /// A savepoint of the currently selected completion. The savepoint
    /// MUST be restored before sending any event to the LSP
    Selected {
        savepoint: Arc<SavePoint>,
    },
    Applied {
        trigger_offset: usize,
        changes: Vec<Change>,
        placeholder: bool,
    },
}

#[derive(Debug, Copy, Clone)]
pub enum Action {
    Load,
    Replace,
    HorizontalSplit,
    VerticalSplit,
}

impl Action {
    /// Whether to align the view to the cursor after executing this action
    pub fn align_view(&self, view: &View, new_doc: DocumentId) -> bool {
        !matches!((self, view.doc == new_doc), (Action::Load, false))
    }
}

/// Error thrown on failed document closed
pub enum CloseError {
    /// Document doesn't exist
    DoesNotExist,
    /// Buffer is modified
    BufferModified(String),
    /// Document failed to save
    SaveError(anyhow::Error),
}

impl Editor {
    pub fn new(
        mut area: Rect,
        theme_loader: Arc<ThemeLoader>,
        syn_loader: Arc<ArcSwap<syntax::Loader>>,
        config: Arc<dyn DynAccess<Config>>,
        handlers: Handlers,
    ) -> Self {
        let language_servers = helix_lsp::Registry::new(syn_loader.clone());
        let conf = config.load();
        let auto_pairs = (&conf.auto_pairs).into();

        // HAXX: offset the render area height by 1 to account for prompt/commandline
        area.height -= 1;

        Self {
            mode: Mode::Normal,
            tree: Tree::new(area),
            next_document_id: DocumentId::default(),
            documents: BTreeMap::new(),
            saves: HashMap::new(),
            save_queue: SelectAll::new(),
            write_count: 0,
            count: None,
            selected_register: None,
            macro_recording: None,
            macro_replaying: Vec::new(),
            theme: theme_loader.default(),
            language_servers,
            diagnostics: Diagnostics::new(),
            diff_providers: DiffProviderRegistry::default(),
            debug_adapters: dap::registry::Registry::new(),
            breakpoints: HashMap::new(),
            syn_loader,
            theme_loader,
            last_theme: None,
            last_selection: None,
            registers: Registers::new(Box::new(arc_swap::access::Map::new(
                Arc::clone(&config),
                |config: &Config| &config.clipboard_provider,
            ))),
            status_msg: None,
            autoinfo: None,
            idle_timer: Box::pin(sleep(conf.idle_timeout)),
            redraw_timer: Box::pin(sleep(Duration::MAX)),
            last_motion: None,
            last_completion: None,
            last_cwd: None,
            config,
            auto_pairs,
            exit_code: 0,
            config_events: unbounded_channel(),
            needs_redraw: false,
            handlers,
            mouse_down_range: None,
            cursor_cache: CursorCache::default(),
            agent: crate::agent::AgentState::new(),
            terminal: crate::terminal::TerminalState::new(),
            git: crate::git::GitState::new(),
            plan: crate::plan::PlanState::default(),
            review: crate::review::ReviewState::default(),
            review_panel: crate::review_panel::ReviewPanelState::default(),
            dir_stack: VecDeque::with_capacity(DIR_STACK_CAP),
        }
    }

    pub fn popup_border(&self) -> bool {
        self.config().popup_border == PopupBorderConfig::All
            || self.config().popup_border == PopupBorderConfig::Popup
    }

    pub fn menu_border(&self) -> bool {
        self.config().popup_border == PopupBorderConfig::All
            || self.config().popup_border == PopupBorderConfig::Menu
    }

    pub fn apply_motion<F: Fn(&mut Self) + 'static>(&mut self, motion: F) {
        motion(self);
        self.last_motion = Some(Box::new(motion));
    }

    pub fn repeat_last_motion(&mut self, count: usize) {
        if let Some(motion) = self.last_motion.take() {
            for _ in 0..count {
                motion(self);
            }
            self.last_motion = Some(motion);
        }
    }
    /// Current editing mode for the [`Editor`].
    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn config(&self) -> DynGuard<Config> {
        self.config.load()
    }

    /// Call if the config has changed to let the editor update all
    /// relevant members.
    pub fn refresh_config(&mut self, old_config: &Config) {
        let config = self.config();
        self.auto_pairs = (&config.auto_pairs).into();
        self.reset_idle_timer();
        self._refresh();
        helix_event::dispatch(crate::events::ConfigDidChange {
            editor: self,
            old: old_config,
            new: &config,
        })
    }

    pub fn clear_idle_timer(&mut self) {
        // equivalent to internal Instant::far_future() (30 years)
        self.idle_timer
            .as_mut()
            .reset(Instant::now() + Duration::from_secs(86400 * 365 * 30));
    }

    pub fn reset_idle_timer(&mut self) {
        let config = self.config();
        self.idle_timer
            .as_mut()
            .reset(Instant::now() + config.idle_timeout);
    }

    pub fn clear_status(&mut self) {
        self.status_msg = None;
    }

    #[inline]
    pub fn set_status<T: Into<Cow<'static, str>>>(&mut self, status: T) {
        let status = status.into();
        log::debug!("editor status: {}", status);
        self.status_msg = Some((status, Severity::Info));
    }

    #[inline]
    pub fn set_error<T: Into<Cow<'static, str>>>(&mut self, error: T) {
        let error = error.into();
        log::debug!("editor error: {}", error);
        self.status_msg = Some((error, Severity::Error));
    }

    #[inline]
    pub fn set_warning<T: Into<Cow<'static, str>>>(&mut self, warning: T) {
        let warning = warning.into();
        log::warn!("editor warning: {}", warning);
        self.status_msg = Some((warning, Severity::Warning));
    }

    #[inline]
    pub fn get_status(&self) -> Option<(&Cow<'static, str>, &Severity)> {
        self.status_msg.as_ref().map(|(status, sev)| (status, sev))
    }

    /// Returns true if the current status is an error
    #[inline]
    pub fn is_err(&self) -> bool {
        self.status_msg
            .as_ref()
            .map(|(_, sev)| *sev == Severity::Error)
            .unwrap_or(false)
    }

    pub(crate) fn _refresh(&mut self) {
        let config = self.config();

        // Reset the inlay hints annotations *before* updating the views, that way we ensure they
        // will disappear during the `.sync_change(doc)` call below.
        //
        // We can't simply check this config when rendering because inlay hints are only parts of
        // the possible annotations, and others could still be active, so we need to selectively
        // drop the inlay hints.
        if !config.lsp.display_inlay_hints {
            for doc in self.documents_mut() {
                doc.reset_all_inlay_hints();
            }
        }

        for (view, _) in self.tree.views_mut() {
            let doc = doc_mut!(self, &view.doc);
            view.sync_changes(doc);
            view.gutters = config.gutters.clone();
            view.ensure_cursor_in_view(doc, config.scrolloff)
        }
    }

    pub fn should_close(&self) -> bool {
        self.tree.is_empty()
    }

    pub fn current_stack_frame(&self) -> Option<&dap::StackFrame> {
        self.debug_adapters.current_stack_frame()
    }

}
