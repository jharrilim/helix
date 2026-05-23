use std::collections::HashMap;
use std::path::{Path, PathBuf};

use agent_client_protocol::schema::{EnvVariable, McpServer, McpServerStdio};
use anyhow::{Context as _, Result};
use serde::Deserialize;

/// Cursor-style MCP configuration file (`.cursor/mcp.json`).
#[derive(Debug, Deserialize)]
struct CursorMcpConfig {
    #[serde(default, rename = "mcpServers")]
    mcp_servers: HashMap<String, CursorMcpServerEntry>,
}

#[derive(Debug, Deserialize)]
struct CursorMcpServerEntry {
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: HashMap<String, String>,
}

/// Resolve MCP servers from Cursor config paths for the given working directory.
pub fn resolve_mcp_servers(cwd: &Path, override_path: Option<&Path>) -> Vec<McpServer> {
    let paths = override_path
        .map(|path| vec![path.to_path_buf()])
        .unwrap_or_else(|| default_mcp_config_paths(cwd));

    for path in paths {
        match load_mcp_servers(&path) {
            Ok(servers) if !servers.is_empty() => return servers,
            Ok(_) => continue,
            Err(err) => {
                log::debug!("failed to load MCP config {}: {err:#}", path.display());
            }
        }
    }

    Vec::new()
}

fn default_mcp_config_paths(cwd: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    paths.push(cwd.join(".cursor").join("mcp.json"));
    if let Ok(home) = helix_stdx::path::home_dir() {
        paths.push(home.join(".cursor").join("mcp.json"));
    }
    paths
}

fn load_mcp_servers(path: &Path) -> Result<Vec<McpServer>> {
    if !path.is_file() {
        return Ok(Vec::new());
    }

    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read MCP config {}", path.display()))?;
    let config: CursorMcpConfig = serde_json::from_str(&contents)
        .with_context(|| format!("failed to parse MCP config {}", path.display()))?;

    Ok(config
        .mcp_servers
        .into_iter()
        .map(|(name, entry)| {
            let env = entry
                .env
                .into_iter()
                .map(|(name, value)| EnvVariable::new(name, value))
                .collect();
            McpServer::Stdio(
                McpServerStdio::new(name, entry.command)
                    .args(entry.args)
                    .env(env),
            )
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn parses_cursor_mcp_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.json");
        let mut file = std::fs::File::create(&path).unwrap();
        write!(
            file,
            r#"{{
  "mcpServers": {{
    "docs": {{
      "command": "/usr/bin/mcp-docs",
      "args": ["--stdio"],
      "env": {{ "TOKEN": "abc" }}
    }}
  }}
}}"#
        )
        .unwrap();

        let servers = load_mcp_servers(&path).unwrap();
        assert_eq!(servers.len(), 1);
        match &servers[0] {
            McpServer::Stdio(server) => {
                assert_eq!(server.name, "docs");
                assert_eq!(server.command, PathBuf::from("/usr/bin/mcp-docs"));
                assert_eq!(server.args, vec!["--stdio".to_string()]);
                assert_eq!(server.env.len(), 1);
            }
            _ => panic!("expected stdio MCP server"),
        }
    }
}
