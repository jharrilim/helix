use std::path::PathBuf;
use std::sync::Arc;

/// Request to create an ACP-backed terminal session.
#[derive(Debug, Clone)]
pub struct TerminalCreateRequest {
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: Option<PathBuf>,
    pub output_byte_limit: Option<u64>,
}

/// Snapshot of terminal output for ACP `terminal/output`.
#[derive(Debug, Clone)]
pub struct TerminalOutputSnapshot {
    pub output: String,
    pub truncated: bool,
    pub exit_code: Option<i32>,
}

/// Result of waiting for a terminal process to exit.
#[derive(Debug, Clone)]
pub struct TerminalExitResult {
    pub exit_code: Option<i32>,
    pub signal: Option<i32>,
}

pub type TerminalCreateFn =
    Arc<dyn Fn(TerminalCreateRequest) -> Result<String, String> + Send + Sync>;
pub type TerminalOutputFn =
    Arc<dyn Fn(String) -> TerminalOutputSnapshot + Send + Sync>;
pub type TerminalWaitExitFn =
    Arc<dyn Fn(String) -> Result<TerminalExitResult, String> + Send + Sync>;
pub type TerminalKillFn = Arc<dyn Fn(String) -> Result<(), String> + Send + Sync>;
pub type TerminalReleaseFn = Arc<dyn Fn(String) -> Result<(), String> + Send + Sync>;

/// Truncate output at a UTF-8 character boundary from the start.
pub fn truncate_output(output: &str, byte_limit: u64) -> (String, bool) {
    if byte_limit == 0 {
        return (String::new(), !output.is_empty());
    }
    if output.len() <= byte_limit as usize {
        return (output.to_string(), false);
    }
    let mut end = byte_limit as usize;
    while end > 0 && !output.is_char_boundary(end) {
        end -= 1;
    }
    (output[end..].to_string(), true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_output_respects_char_boundary() {
        let text = "hello 🌍 world";
        let (truncated, was_truncated) = truncate_output(text, 8);
        assert!(was_truncated);
        assert!(truncated.is_char_boundary(truncated.len()));
    }
}
