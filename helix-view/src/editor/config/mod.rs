mod gutter;
mod lsp;
mod misc;
mod pickers;
mod statusline;
mod whitespace;

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use crate::{
    annotations::diagnostics::{DiagnosticFilter, InlineDiagnosticsConfig},
    clipboard::ClipboardProvider,
};
use helix_core::syntax::config::{AutoPairConfig, IndentationHeuristic, SoftWrap};
use helix_core::diagnostic::Severity;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub use gutter::{
    GutterConfig, GutterLineNumbersConfig, GutterType, LineNumber,
};
pub use lsp::{LspConfig, SearchConfig};
pub use misc::{
    get_terminal_provider, AutoSave, AutoSaveAfterDelay, BufferLine, CursorShapeConfig,
    KittyKeyboardProtocolConfig, LineEndingConfig, PopupBorderConfig, SmartTabConfig,
    TerminalConfig, WordCompletion,
};
pub use pickers::{
    BufferPickerConfig, FileExplorerConfig, FilePickerConfig, PickerStartPosition,
};
pub use statusline::{ModeConfig, StatusLineConfig, StatusLineElement};
pub use whitespace::{
    IndentGuidesConfig, WhitespaceCharacters, WhitespaceConfig, WhitespaceRender,
    WhitespaceRenderValue,
};

pub const DEFAULT_AUTO_SAVE_DELAY: u64 = 3000;

pub(crate) fn deserialize_duration_millis<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let millis = u64::deserialize(deserializer)?;
    Ok(Duration::from_millis(millis))
}

pub(crate) fn serialize_duration_millis<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_u64(
        duration
            .as_millis()
            .try_into()
            .map_err(|_| serde::ser::Error::custom("duration value overflowed u64"))?,
    )
}

pub(crate) fn serialize_alphabet<S>(alphabet: &[char], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let alphabet: String = alphabet.iter().collect();
    serializer.serialize_str(&alphabet)
}

pub(crate) fn deserialize_alphabet<'de, D>(deserializer: D) -> Result<Vec<char>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;

    let str = String::deserialize(deserializer)?;
    let chars: Vec<_> = str.chars().collect();
    let unique_chars: HashSet<_> = chars.iter().copied().collect();
    if unique_chars.len() != chars.len() {
        return Err(<D::Error as Error>::custom(
            "jump-label-alphabet must contain unique characters",
        ));
    }
    Ok(chars)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", default, deny_unknown_fields)]
pub struct Config {
    /// Padding to keep between the edge of the screen and the cursor when scrolling. Defaults to 5.
    pub scrolloff: usize,
    /// Number of lines to scroll at once. Defaults to 3
    pub scroll_lines: isize,
    /// Mouse support. Defaults to true.
    pub mouse: bool,
    /// Which register to use for mouse yank.
    pub mouse_yank_register: char,
    /// Shell to use for shell commands. Defaults to ["cmd", "/C"] on Windows and ["sh", "-c"] otherwise.
    pub shell: Vec<String>,
    /// Line number mode.
    pub line_number: LineNumber,
    /// Highlight the lines cursors are currently on. Defaults to false.
    pub cursorline: bool,
    /// Highlight the columns cursors are currently on. Defaults to false.
    pub cursorcolumn: bool,
    #[serde(deserialize_with = "gutter::deserialize_gutter_seq_or_struct")]
    pub gutters: GutterConfig,
    /// Middle click paste support. Defaults to true.
    pub middle_click_paste: bool,
    /// Automatic insertion of pairs to parentheses, brackets,
    /// etc. Optionally, this can be a list of 2-tuples to specify a
    /// global list of characters to pair. Defaults to true.
    pub auto_pairs: AutoPairConfig,
    /// Automatic auto-completion, automatically pop up without user trigger. Defaults to true.
    pub auto_completion: bool,
    /// Enable filepath completion.
    /// Show files and directories if an existing path at the cursor was recognized,
    /// either absolute or relative to the current opened document or current working directory (if the buffer is not yet saved).
    /// Defaults to true.
    pub path_completion: bool,
    /// Configures completion of words from open buffers.
    /// Defaults to enabled with a trigger length of 7.
    pub word_completion: WordCompletion,
    /// Automatic formatting on save. Defaults to true.
    pub auto_format: bool,
    /// Default register used for yank/paste. Defaults to '"'
    pub default_yank_register: char,
    /// Automatic save on focus lost and/or after delay.
    /// Time delay in milliseconds since last edit after which auto save timer triggers.
    /// Time delay defaults to false with 3000ms delay. Focus lost defaults to false.
    #[serde(deserialize_with = "misc::deserialize_auto_save")]
    pub auto_save: AutoSave,
    /// Set a global text_width
    pub text_width: usize,
    /// Time in milliseconds since last keypress before idle timers trigger.
    /// Used for various UI timeouts. Defaults to 250ms.
    #[serde(
        serialize_with = "serialize_duration_millis",
        deserialize_with = "deserialize_duration_millis"
    )]
    pub idle_timeout: Duration,
    /// Time in milliseconds after typing a word character before auto completions
    /// are shown, set to 5 for instant. Defaults to 250ms.
    #[serde(
        serialize_with = "serialize_duration_millis",
        deserialize_with = "deserialize_duration_millis"
    )]
    pub completion_timeout: Duration,
    /// Whether to insert the completion suggestion on hover. Defaults to true.
    pub preview_completion_insert: bool,
    pub completion_trigger_len: u8,
    /// Whether to instruct the LSP to replace the entire word when applying a completion
    /// or to only insert new text
    pub completion_replace: bool,
    /// `true` if helix should automatically add a line comment token if you're currently in a comment
    /// and press `enter`.
    pub continue_comments: bool,
    /// Whether to display infoboxes. Defaults to true.
    pub auto_info: bool,
    pub file_picker: FilePickerConfig,
    pub file_explorer: FileExplorerConfig,
    /// Configuration of the statusline elements
    pub statusline: StatusLineConfig,
    /// Shape for cursor in each mode
    pub cursor_shape: CursorShapeConfig,
    /// Set to `true` to override automatic detection of terminal truecolor support in the event of a false negative. Defaults to `false`.
    pub true_color: bool,
    /// Set to `true` to override automatic detection of terminal undercurl support in the event of a false negative. Defaults to `false`.
    pub undercurl: bool,
    /// Search configuration.
    #[serde(default)]
    pub search: SearchConfig,
    pub lsp: LspConfig,
    pub terminal: Option<TerminalConfig>,
    /// Column numbers at which to draw the rulers. Defaults to `[]`, meaning no rulers.
    pub rulers: Vec<u16>,
    #[serde(default)]
    pub whitespace: WhitespaceConfig,
    /// Persistently display open buffers along the top
    pub bufferline: BufferLine,
    /// Vertical indent width guides.
    pub indent_guides: IndentGuidesConfig,
    /// Whether to color modes with different colors. Defaults to `false`.
    pub color_modes: bool,
    pub soft_wrap: SoftWrap,
    /// Workspace specific lsp ceiling dirs
    pub workspace_lsp_roots: Vec<PathBuf>,
    /// Which line ending to choose for new documents. Defaults to `native`. i.e. `crlf` on Windows, otherwise `lf`.
    pub default_line_ending: LineEndingConfig,
    /// Whether to automatically insert a trailing line-ending on write if missing. Defaults to `true`.
    pub insert_final_newline: bool,
    /// Whether to use atomic operations to write documents to disk.
    /// This prevents data loss if the editor is interrupted while writing the file, but may
    /// confuse some file watching/hot reloading programs. Defaults to `true`.
    pub atomic_save: bool,
    /// Whether to automatically remove all trailing line-endings after the final one on write.
    /// Defaults to `false`.
    pub trim_final_newlines: bool,
    /// Whether to automatically remove all whitespace characters preceding line-endings on write.
    /// Defaults to `false`.
    pub trim_trailing_whitespace: bool,
    /// Enables smart tab
    pub smart_tab: Option<SmartTabConfig>,
    /// Draw border around popups.
    pub popup_border: PopupBorderConfig,
    /// Which indent heuristic to use when a new line is inserted
    #[serde(default)]
    pub indent_heuristic: IndentationHeuristic,
    /// labels characters used in jumpmode
    #[serde(
        serialize_with = "serialize_alphabet",
        deserialize_with = "deserialize_alphabet"
    )]
    pub jump_label_alphabet: Vec<char>,
    /// Display diagnostic below the line they occur.
    pub inline_diagnostics: InlineDiagnosticsConfig,
    pub end_of_line_diagnostics: DiagnosticFilter,
    // Set to override the default clipboard provider
    pub clipboard_provider: ClipboardProvider,
    /// Whether to read settings from [EditorConfig](https://editorconfig.org) files. Defaults to
    /// `true`.
    pub editor_config: bool,
    /// Whether to render rainbow colors for matching brackets. Defaults to `false`.
    pub rainbow_brackets: bool,
    /// Whether to enable Kitty Keyboard Protocol
    pub kitty_keyboard_protocol: KittyKeyboardProtocolConfig,
    pub buffer_picker: BufferPickerConfig,
    /// Whether to implicitly trust every workspace or not
    pub insecure: bool,
    /// ACP agent integration settings.
    #[serde(default)]
    pub agent: crate::agent::AgentSettings,
    /// Integrated terminal settings.
    #[serde(default)]
    pub integrated_terminal: crate::terminal::TerminalSettings,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            scrolloff: 5,
            scroll_lines: 3,
            mouse: true,
            mouse_yank_register: '*',
            shell: if cfg!(windows) {
                vec!["cmd".to_owned(), "/C".to_owned()]
            } else {
                vec!["sh".to_owned(), "-c".to_owned()]
            },
            line_number: LineNumber::Absolute,
            cursorline: false,
            cursorcolumn: false,
            gutters: GutterConfig::default(),
            middle_click_paste: true,
            auto_pairs: AutoPairConfig::default(),
            auto_completion: true,
            path_completion: true,
            word_completion: WordCompletion::default(),
            auto_format: true,
            default_yank_register: '"',
            auto_save: AutoSave::default(),
            idle_timeout: Duration::from_millis(250),
            completion_timeout: Duration::from_millis(250),
            preview_completion_insert: true,
            completion_trigger_len: 2,
            auto_info: true,
            file_picker: FilePickerConfig::default(),
            file_explorer: FileExplorerConfig::default(),
            statusline: StatusLineConfig::default(),
            cursor_shape: CursorShapeConfig::default(),
            true_color: false,
            undercurl: false,
            search: SearchConfig::default(),
            lsp: LspConfig::default(),
            terminal: get_terminal_provider(),
            rulers: Vec::new(),
            whitespace: WhitespaceConfig::default(),
            bufferline: BufferLine::default(),
            indent_guides: IndentGuidesConfig::default(),
            color_modes: false,
            soft_wrap: SoftWrap {
                enable: Some(false),
                ..SoftWrap::default()
            },
            text_width: 80,
            completion_replace: false,
            continue_comments: true,
            workspace_lsp_roots: Vec::new(),
            default_line_ending: LineEndingConfig::default(),
            insert_final_newline: true,
            atomic_save: true,
            trim_final_newlines: false,
            trim_trailing_whitespace: false,
            smart_tab: Some(SmartTabConfig::default()),
            popup_border: PopupBorderConfig::None,
            indent_heuristic: IndentationHeuristic::default(),
            jump_label_alphabet: ('a'..='z').collect(),
            inline_diagnostics: InlineDiagnosticsConfig::default(),
            end_of_line_diagnostics: DiagnosticFilter::Enable(Severity::Hint),
            clipboard_provider: ClipboardProvider::default(),
            editor_config: true,
            rainbow_brackets: false,
            kitty_keyboard_protocol: Default::default(),
            buffer_picker: BufferPickerConfig::default(),
            insecure: false,
            agent: crate::agent::AgentSettings::default(),
            integrated_terminal: crate::terminal::TerminalSettings::default(),
        }
    }
}
