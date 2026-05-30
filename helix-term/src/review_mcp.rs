//! Stdio MCP server exposing code-review tools.

use std::io::{self, BufRead, BufReader, Write};

use std::path::PathBuf;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::commands::review::{append_agent_review_reply_to_review, AgentReviewReplyInput};

const MCP_TOOL_REVIEW_REPLY: &str = "review_reply";

#[derive(Debug, Clone)]
pub struct ReviewReplyCliArgs {
    pub review_id: String,
    pub comment_id: Option<String>,
    pub file_path: Option<PathBuf>,
    pub line: Option<usize>,
    pub body: String,
}

#[derive(Debug, Clone)]
pub struct ReviewReplyCliOutput {
    pub review_id: String,
    pub comment_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReviewReplyToolArgs {
    #[serde(default)]
    review_id: Option<String>,
    #[serde(default)]
    comment_id: Option<String>,
    #[serde(default)]
    file_path: Option<String>,
    #[serde(default)]
    line: Option<usize>,
    body: String,
}

/// Run Helix review MCP server over stdio.
pub fn run_stdio_server() -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = stdout.lock();

    while let Some(message) = read_jsonrpc_message(&mut reader)? {
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            continue;
        };
        let id = message.get("id").cloned();
        let params = message.get("params").cloned().unwrap_or_else(|| json!({}));

        if id.is_none() {
            continue;
        }
        let id = id.expect("checked above");

        match method {
            "initialize" => {
                write_jsonrpc_response(
                    &mut writer,
                    id,
                    json!({
                        "protocolVersion": "2024-11-05",
                        "capabilities": {
                            "tools": {}
                        },
                        "serverInfo": {
                            "name": "helix-review-mcp",
                            "version": env!("CARGO_PKG_VERSION")
                        }
                    }),
                )?;
            }
            "ping" => {
                write_jsonrpc_response(&mut writer, id, json!({}))?;
            }
            "tools/list" => {
                write_jsonrpc_response(
                    &mut writer,
                    id,
                    json!({
                        "tools": [
                            {
                                "name": MCP_TOOL_REVIEW_REPLY,
                                "description": "Add an agent-authored code-review reply comment",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "reviewId": { "type": "string" },
                                        "commentId": { "type": "string" },
                                        "filePath": { "type": "string" },
                                        "line": { "type": "integer", "minimum": 0 },
                                        "body": { "type": "string" }
                                    },
                                    "required": ["reviewId", "body"],
                                    "oneOf": [
                                        { "required": ["commentId"] },
                                        { "required": ["filePath", "line"] }
                                    ]
                                }
                            }
                        ]
                    }),
                )?;
            }
            "tools/call" => {
                let result = handle_tools_call(params);
                match result {
                    Ok(value) => write_jsonrpc_response(&mut writer, id, value)?,
                    Err(err) => write_jsonrpc_response(
                        &mut writer,
                        id,
                        json!({
                            "content": [{ "type": "text", "text": err }],
                            "isError": true
                        }),
                    )?,
                }
            }
            _ => {
                write_jsonrpc_error(
                    &mut writer,
                    id,
                    -32601,
                    format!("method not found: {method}"),
                )?;
            }
        }
    }

    Ok(())
}

pub fn run_review_reply_cli(args: ReviewReplyCliArgs) -> Result<ReviewReplyCliOutput, String> {
    if args.body.trim().is_empty() {
        return Err("review reply body is empty".into());
    }
    if args.comment_id.is_none() && (args.file_path.is_none() || args.line.is_none()) {
        return Err("either --comment-id or --file-path with --line is required".into());
    }

    let repo_slug = helix_review::repo_slug();
    let (review_id, comment_id) = execute_review_reply_for_repo(
        &repo_slug,
        AgentReviewReplyInput {
            review_id: Some(args.review_id),
            comment_id: args.comment_id,
            file_path: args.file_path,
            line: args.line,
            body: args.body,
        },
    )?;

    Ok(ReviewReplyCliOutput {
        review_id,
        comment_id,
    })
}

fn handle_tools_call(params: Value) -> Result<Value, String> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "tools/call missing name".to_string())?;
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    if name != MCP_TOOL_REVIEW_REPLY {
        return Err(format!("unsupported tool: {name}"));
    }

    let args = serde_json::from_value::<ReviewReplyToolArgs>(arguments)
        .map_err(|err| format!("invalid review_reply arguments: {err}"))?;
    let body = args.body.trim().to_string();
    if body.is_empty() {
        return Err("review reply body is empty".into());
    }
    let review_id = args
        .review_id
        .clone()
        .ok_or_else(|| "reviewId is required; pass the active review id".to_string())?;

    let repo_slug = helix_review::repo_slug();
    let (review_id, comment_id) = execute_review_reply_for_repo(
        &repo_slug,
        AgentReviewReplyInput {
            review_id: Some(review_id),
            comment_id: args.comment_id,
            file_path: args.file_path.map(std::path::PathBuf::from),
            line: args.line,
            body,
        },
    )?;

    Ok(json!({
        "content": [{
            "type": "text",
            "text": format!("Added review reply {} to review {}", comment_id, review_id)
        }],
        "structuredContent": {
            "commentId": comment_id,
            "reviewId": review_id
        }
    }))
}

pub(crate) fn execute_review_reply_for_repo(
    repo_slug: &str,
    input: AgentReviewReplyInput,
) -> Result<(String, String), String> {
    let mut review = load_review_for_reply(repo_slug, input.review_id.clone())?;
    let review_id = review.metadata.id.clone();
    let comment_id = append_agent_review_reply_to_review(&mut review, &input)?;
    helix_review::save_review(repo_slug, &review)
        .map_err(|err| format!("failed to save review: {err:#}"))?;
    Ok((review_id, comment_id))
}

fn load_review_for_reply(
    repo_slug: &str,
    review_id: Option<String>,
) -> Result<helix_review::ReviewData, String> {
    let review_id =
        review_id.ok_or_else(|| "reviewId is required; pass the active review id".to_string())?;
    helix_review::load_review(repo_slug, &review_id)
        .map_err(|err| format!("failed to load review '{review_id}': {err:#}"))
}

fn read_jsonrpc_message<R: BufRead>(reader: &mut R) -> io::Result<Option<Value>> {
    let mut content_length: Option<usize> = None;
    let mut line = String::new();
    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line)?;
        if bytes_read == 0 {
            return Ok(None);
        }

        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some(value) = trimmed.strip_prefix("Content-Length:") {
            content_length = value.trim().parse::<usize>().ok();
        }
    }

    let content_length = content_length
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing Content-Length"))?;
    let mut payload = vec![0u8; content_length];
    reader.read_exact(&mut payload)?;

    let message = serde_json::from_slice::<Value>(&payload).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid JSON-RPC payload: {err}"),
        )
    })?;
    Ok(Some(message))
}

fn write_jsonrpc_response<W: Write>(writer: &mut W, id: Value, result: Value) -> io::Result<()> {
    let message = json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    });
    write_jsonrpc_message(writer, &message)
}

fn write_jsonrpc_error<W: Write>(
    writer: &mut W,
    id: Value,
    code: i64,
    message: String,
) -> io::Result<()> {
    let payload = json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    });
    write_jsonrpc_message(writer, &payload)
}

fn write_jsonrpc_message<W: Write>(writer: &mut W, message: &Value) -> io::Result<()> {
    let bytes = serde_json::to_vec(message).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to serialize JSON-RPC message: {err}"),
        )
    })?;
    write!(writer, "Content-Length: {}\r\n\r\n", bytes.len())?;
    writer.write_all(&bytes)?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use helix_review::{create_new_review, load_review, save_review, timestamp_now, CommentAuthor, ReviewComment};
    use serde_json::json;
    use tempfile::TempDir;

    fn setup_review_with_parent_comment() -> (TempDir, String, String) {
        let temp = TempDir::new().expect("tempdir");
        let repo_root = temp.path().join("repo");
        std::fs::create_dir_all(repo_root.join("src")).expect("create repo tree");
        std::fs::write(repo_root.join("src/lib.rs"), "fn demo() {}\n").expect("write file");

        let slug = format!("review-mcp-test-{}", timestamp_now());
        let mut review = create_new_review(&repo_root, &slug, "MCP test review");
        review.comments.push(ReviewComment {
            id: "parent-comment".into(),
            file: PathBuf::from("src/lib.rs"),
            line: 0,
            line_end: None,
            char_idx: 0,
            body: "existing".into(),
            author: CommentAuthor::User,
            context_before: vec![],
            context_after: vec![],
            code_at_comment: String::new(),
            diff_side: None,
            hunk_index: None,
            created_at: timestamp_now(),
        });
        let review_id = review.metadata.id.clone();
        save_review(&slug, &review).expect("save review");
        (temp, slug, review_id)
    }

    #[test]
    fn handles_tools_call_requires_tool_name() {
        let err = handle_tools_call(json!({ "arguments": {} })).expect_err("missing name");
        assert!(err.contains("missing name"));
    }

    #[test]
    fn review_reply_tool_by_comment_id() {
        let (_temp, slug, review_id) = setup_review_with_parent_comment();
        let (returned_review_id, inserted_id) = execute_review_reply_for_repo(
            &slug,
            AgentReviewReplyInput {
                review_id: Some(review_id.clone()),
                comment_id: Some("parent-comment".into()),
                file_path: None,
                line: None,
                body: "Agent reply from MCP".into(),
            },
        )
        .expect("tool call should succeed");
        assert_eq!(returned_review_id, review_id);
        assert!(!inserted_id.is_empty());

        let saved = load_review(&slug, &review_id).expect("load saved review");
        let inserted = saved
            .comments
            .iter()
            .find(|comment| comment.id == inserted_id)
            .expect("inserted comment");
        assert_eq!(inserted.author, CommentAuthor::Agent);
        assert_eq!(inserted.body, "Agent reply from MCP");
    }

    #[test]
    fn review_reply_tool_invalid_comment_id_errors() {
        let (_temp, slug, _review_id) = setup_review_with_parent_comment();
        let err = execute_review_reply_for_repo(
            &slug,
            AgentReviewReplyInput {
                review_id: Some("missing-review".into()),
                comment_id: Some("missing".into()),
                file_path: None,
                line: None,
                body: "Agent reply".into(),
            },
        )
        .expect_err("tool should reject missing review id");
        assert!(err.contains("failed to load review"));
    }

    #[test]
    fn review_reply_tool_requires_review_id() {
        let (_temp, slug, _review_id) = setup_review_with_parent_comment();
        let err = execute_review_reply_for_repo(
            &slug,
            AgentReviewReplyInput {
                review_id: None,
                comment_id: Some("missing".into()),
                file_path: None,
                line: None,
                body: "Agent reply".into(),
            },
        )
        .expect_err("tool should require review id");
        assert!(err.contains("reviewId is required"));
    }
}
