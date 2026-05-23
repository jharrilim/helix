use std::path::PathBuf;

/// Result of reading a file on behalf of an ACP agent.
#[derive(Debug, Clone)]
pub struct FsReadResult {
    pub content: String,
}

/// Request to write a file on behalf of an ACP agent.
#[derive(Debug, Clone)]
pub struct FsWriteRequest {
    pub path: PathBuf,
    pub content: String,
}

/// Result of applying an agent file write.
#[derive(Debug, Clone)]
pub enum FsWriteResult {
    Applied,
    /// Buffer has unsaved user edits; agent must wait for approval.
    Conflict {
        message: String,
    },
    /// User rejected the write.
    Rejected,
    Error(String),
}
