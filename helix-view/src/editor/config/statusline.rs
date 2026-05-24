use helix_core::diagnostic::Severity;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", default, deny_unknown_fields)]
pub struct StatusLineConfig {
    pub left: Vec<StatusLineElement>,
    pub center: Vec<StatusLineElement>,
    pub right: Vec<StatusLineElement>,
    pub separator: String,
    pub mode: ModeConfig,
    pub diagnostics: Vec<Severity>,
    pub workspace_diagnostics: Vec<Severity>,
}

impl Default for StatusLineConfig {
    fn default() -> Self {
        use StatusLineElement as E;

        Self {
            left: vec![
                E::Mode,
                E::Spinner,
                E::FileName,
                E::ReadOnlyIndicator,
                E::FileModificationIndicator,
            ],
            center: vec![],
            right: vec![
                E::Diagnostics,
                E::Selections,
                E::Register,
                E::Position,
                E::FileEncoding,
            ],
            separator: String::from("│"),
            mode: ModeConfig::default(),
            diagnostics: vec![Severity::Warning, Severity::Error],
            workspace_diagnostics: vec![Severity::Warning, Severity::Error],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", default, deny_unknown_fields)]
pub struct ModeConfig {
    pub normal: String,
    pub insert: String,
    pub select: String,
}

impl Default for ModeConfig {
    fn default() -> Self {
        Self {
            normal: String::from("NOR"),
            insert: String::from("INS"),
            select: String::from("SEL"),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StatusLineElement {
    /// The editor mode (Normal, Insert, Visual/Selection)
    Mode,

    /// The LSP activity spinner
    Spinner,

    /// The file basename (the leaf of the open file's path)
    FileBaseName,

    /// The relative file path
    FileName,

    /// The file absolute path
    FileAbsolutePath,

    // The file modification indicator
    FileModificationIndicator,

    /// An indicator that shows `"[readonly]"` when a file cannot be written
    ReadOnlyIndicator,

    /// The file encoding
    FileEncoding,

    /// The file line endings (CRLF or LF)
    FileLineEnding,

    /// The file indentation style
    FileIndentStyle,

    /// The file type (language ID or "text")
    FileType,

    /// A summary of the number of errors and warnings
    Diagnostics,

    /// A summary of the number of errors and warnings on file and workspace
    WorkspaceDiagnostics,

    /// The number of selections (cursors)
    Selections,

    /// The number of characters currently in primary selection
    PrimarySelectionLength,

    /// The cursor position
    Position,

    /// The separator string
    Separator,

    /// The cursor position as a percent of the total file
    PositionPercentage,

    /// The total line numbers of the current file
    TotalLineNumbers,

    /// A single space
    Spacer,

    /// Current version control information
    VersionControl,

    /// Indicator for selected register
    Register,

    /// The base of current working directory
    CurrentWorkingDirectory,
}
