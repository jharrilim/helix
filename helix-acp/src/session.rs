use std::fmt;
use std::path::PathBuf;

/// Opaque ACP session identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AgentSessionId(pub String);

impl fmt::Display for AgentSessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for AgentSessionId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// Metadata for a session returned by ACP `session/list`.
#[derive(Debug, Clone)]
pub struct AgentSessionInfo {
    pub id: AgentSessionId,
    pub title: Option<String>,
    pub cwd: PathBuf,
    pub updated_at: Option<String>,
}
