//! Agent Client Protocol (ACP) integration for Helix.

// Hello!

mod cursor;
mod events;
mod fs;
mod mcp_config;
mod runtime;
mod session;

pub use cursor::*;
pub use events::*;
pub use mcp_config::resolve_mcp_servers;
pub use fs::{FsReadResult, FsWriteRequest, FsWriteResult};
pub use runtime::{AgentConfig, AgentRuntime, AgentRuntimeHandle, FsReadFn, FsWriteFn};
pub use session::{AgentSessionId, AgentSessionInfo};
