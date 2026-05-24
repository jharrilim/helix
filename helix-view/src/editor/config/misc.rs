use std::collections::HashMap;
use std::num::NonZeroU8;

use crate::{
    document::Mode,
    graphics::CursorKind,
};
use helix_core::{
    LineEnding, NATIVE_LINE_ENDING,
};
use serde::{ser::SerializeMap, Deserialize, Deserializer, Serialize, Serializer};

use super::DEFAULT_AUTO_SAVE_DELAY;

#[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum KittyKeyboardProtocolConfig {
    #[default]
    Auto,
    Disabled,
    Enabled,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Eq, PartialOrd, Ord)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct SmartTabConfig {
    pub enable: bool,
    pub supersede_menu: bool,
}

impl Default for SmartTabConfig {
    fn default() -> Self {
        SmartTabConfig {
            enable: true,
            supersede_menu: false,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct TerminalConfig {
    pub command: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
}

#[cfg(windows)]
pub fn get_terminal_provider() -> Option<TerminalConfig> {
    use helix_stdx::env::binary_exists;

    if binary_exists("wt") {
        return Some(TerminalConfig {
            command: "wt".to_string(),
            args: vec![
                "new-tab".to_string(),
                "--title".to_string(),
                "DEBUG".to_string(),
                "cmd".to_string(),
                "/C".to_string(),
            ],
        });
    }

    Some(TerminalConfig {
        command: "conhost".to_string(),
        args: vec!["cmd".to_string(), "/C".to_string()],
    })
}

#[cfg(not(any(windows, target_arch = "wasm32")))]
pub fn get_terminal_provider() -> Option<TerminalConfig> {
    use helix_stdx::env::{binary_exists, env_var_is_set};

    if env_var_is_set("TMUX") && binary_exists("tmux") {
        return Some(TerminalConfig {
            command: "tmux".to_string(),
            args: vec!["split-window".to_string()],
        });
    }

    if env_var_is_set("WEZTERM_UNIX_SOCKET") && binary_exists("wezterm") {
        return Some(TerminalConfig {
            command: "wezterm".to_string(),
            args: vec!["cli".to_string(), "split-pane".to_string()],
        });
    }

    None
}

// Cursor shape is read and used on every rendered frame and so needs
// to be fast. Therefore we avoid a hashmap and use an enum indexed array.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorShapeConfig([CursorKind; 3]);

impl CursorShapeConfig {
    pub fn from_mode(&self, mode: Mode) -> CursorKind {
        self.get(mode as usize).copied().unwrap_or_default()
    }
}

impl<'de> Deserialize<'de> for CursorShapeConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let m = HashMap::<Mode, CursorKind>::deserialize(deserializer)?;
        let into_cursor = |mode: Mode| m.get(&mode).copied().unwrap_or_default();
        Ok(CursorShapeConfig([
            into_cursor(Mode::Normal),
            into_cursor(Mode::Select),
            into_cursor(Mode::Insert),
        ]))
    }
}

impl Serialize for CursorShapeConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.len()))?;
        let modes = [Mode::Normal, Mode::Select, Mode::Insert];
        for mode in modes {
            map.serialize_entry(&mode, &self.from_mode(mode))?;
        }
        map.end()
    }
}

impl std::ops::Deref for CursorShapeConfig {
    type Target = [CursorKind; 3];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Default for CursorShapeConfig {
    fn default() -> Self {
        Self([CursorKind::Block; 3])
    }
}

/// bufferline render modes
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BufferLine {
    /// Don't render bufferline
    #[default]
    Never,
    /// Always render
    Always,
    /// Only if multiple buffers are open
    Multiple,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct AutoSave {
    /// Auto save after a delay in milliseconds. Defaults to disabled.
    #[serde(default)]
    pub after_delay: AutoSaveAfterDelay,
    /// Auto save on focus lost. Defaults to false.
    #[serde(default)]
    pub focus_lost: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AutoSaveAfterDelay {
    #[serde(default)]
    /// Enable auto save after delay. Defaults to false.
    pub enable: bool,
    #[serde(default = "default_auto_save_delay")]
    /// Time delay in milliseconds. Defaults to [DEFAULT_AUTO_SAVE_DELAY].
    pub timeout: u64,
}

impl Default for AutoSaveAfterDelay {
    fn default() -> Self {
        Self {
            enable: false,
            timeout: DEFAULT_AUTO_SAVE_DELAY,
        }
    }
}

fn default_auto_save_delay() -> u64 {
    DEFAULT_AUTO_SAVE_DELAY
}

pub(crate) fn deserialize_auto_save<'de, D>(deserializer: D) -> Result<AutoSave, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize, Serialize)]
    #[serde(untagged, deny_unknown_fields, rename_all = "kebab-case")]
    enum AutoSaveToml {
        EnableFocusLost(bool),
        AutoSave(AutoSave),
    }

    match AutoSaveToml::deserialize(deserializer)? {
        AutoSaveToml::EnableFocusLost(focus_lost) => Ok(AutoSave {
            focus_lost,
            ..Default::default()
        }),
        AutoSaveToml::AutoSave(auto_save) => Ok(auto_save),
    }
}

/// Line ending configuration.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LineEndingConfig {
    /// The platform's native line ending.
    ///
    /// `crlf` on Windows, otherwise `lf`.
    #[default]
    Native,
    /// Line feed.
    LF,
    /// Carriage return followed by line feed.
    Crlf,
    /// Form feed.
    #[cfg(feature = "unicode-lines")]
    FF,
    /// Carriage return.
    #[cfg(feature = "unicode-lines")]
    CR,
    /// Next line.
    #[cfg(feature = "unicode-lines")]
    Nel,
}

impl From<LineEndingConfig> for LineEnding {
    fn from(line_ending: LineEndingConfig) -> Self {
        match line_ending {
            LineEndingConfig::Native => NATIVE_LINE_ENDING,
            LineEndingConfig::LF => LineEnding::LF,
            LineEndingConfig::Crlf => LineEnding::Crlf,
            #[cfg(feature = "unicode-lines")]
            LineEndingConfig::FF => LineEnding::FF,
            #[cfg(feature = "unicode-lines")]
            LineEndingConfig::CR => LineEnding::CR,
            #[cfg(feature = "unicode-lines")]
            LineEndingConfig::Nel => LineEnding::Nel,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PopupBorderConfig {
    None,
    All,
    Popup,
    Menu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct WordCompletion {
    pub enable: bool,
    pub trigger_length: NonZeroU8,
}

impl Default for WordCompletion {
    fn default() -> Self {
        Self {
            enable: true,
            trigger_length: NonZeroU8::new(7).unwrap(),
        }
    }
}
