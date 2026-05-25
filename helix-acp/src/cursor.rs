//! Cursor ACP extension method types and JSON-RPC matching helpers.

use agent_client_protocol::{Error, JsonRpcMessage, JsonRpcNotification, JsonRpcRequest};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Incoming Cursor extension request (`cursor/*` methods with a JSON-RPC id).
#[derive(Debug, Clone)]
pub struct CursorExtensionRequest {
    pub method: String,
    pub params: Value,
}

impl JsonRpcMessage for CursorExtensionRequest {
    fn matches_method(method: &str) -> bool {
        method.starts_with("cursor/")
    }

    fn method(&self) -> &str {
        &self.method
    }

    fn to_untyped_message(&self) -> Result<agent_client_protocol::UntypedMessage, Error> {
        agent_client_protocol::UntypedMessage::new(&self.method, &self.params)
    }

    fn parse_message(method: &str, params: &impl Serialize) -> Result<Self, Error> {
        if !Self::matches_method(method) {
            return Err(Error::method_not_found());
        }
        Ok(Self {
            method: method.to_string(),
            params: agent_client_protocol::util::json_cast(params)?,
        })
    }
}

impl JsonRpcRequest for CursorExtensionRequest {
    type Response = Value;
}

/// Incoming Cursor extension notification (`cursor/*` without expecting a response).
#[derive(Debug, Clone)]
pub struct CursorExtensionNotification {
    pub method: String,
    pub params: Value,
}

impl JsonRpcMessage for CursorExtensionNotification {
    fn matches_method(method: &str) -> bool {
        method.starts_with("cursor/")
    }

    fn method(&self) -> &str {
        &self.method
    }

    fn to_untyped_message(&self) -> Result<agent_client_protocol::UntypedMessage, Error> {
        agent_client_protocol::UntypedMessage::new(&self.method, &self.params)
    }

    fn parse_message(method: &str, params: &impl Serialize) -> Result<Self, Error> {
        if !Self::matches_method(method) {
            return Err(Error::method_not_found());
        }
        Ok(Self {
            method: method.to_string(),
            params: agent_client_protocol::util::json_cast(params)?,
        })
    }
}

impl JsonRpcNotification for CursorExtensionNotification {}

pub const METHOD_ASK_QUESTION: &str = "cursor/ask_question";
pub const METHOD_CREATE_PLAN: &str = "cursor/create_plan";
pub const METHOD_UPDATE_TODOS: &str = "cursor/update_todos";
pub const METHOD_TASK: &str = "cursor/task";
pub const METHOD_GENERATE_IMAGE: &str = "cursor/generate_image";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorAskQuestionRequest {
    pub tool_call_id: String,
    pub title: Option<String>,
    pub questions: Vec<CursorQuestion>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorQuestion {
    pub id: String,
    pub prompt: String,
    pub options: Vec<CursorQuestionOption>,
    #[serde(default)]
    pub allow_multiple: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorQuestionOption {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorAskQuestionResponse {
    pub outcome: CursorAskQuestionOutcome,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "outcome", rename_all = "camelCase")]
pub enum CursorAskQuestionOutcome {
    #[serde(rename_all = "camelCase")]
    Answered {
        answers: Vec<CursorQuestionAnswer>,
    },
    #[serde(rename_all = "camelCase")]
    Skipped {
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorQuestionAnswer {
    pub question_id: String,
    pub selected_option_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorCreatePlanRequest {
    pub tool_call_id: String,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub plan: String,
    pub todos: Vec<CursorTodo>,
    #[serde(default)]
    pub is_project: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorTodo {
    pub id: String,
    pub content: String,
    pub status: CursorTodoStatus,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CursorTodoStatus {
    Pending,
    InProgress,
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorCreatePlanResponse {
    pub outcome: CursorCreatePlanOutcome,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "outcome", rename_all = "camelCase")]
pub enum CursorCreatePlanOutcome {
    #[serde(rename_all = "camelCase")]
    Accepted {
        #[serde(skip_serializing_if = "Option::is_none")]
        plan_uri: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Rejected {
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    Cancelled,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorUpdateTodosRequest {
    pub tool_call_id: String,
    pub todos: Vec<CursorTodo>,
    pub merge: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorTaskRequest {
    pub tool_call_id: String,
    pub description: String,
    pub prompt: String,
    pub subagent_type: Value,
    pub model: Option<String>,
    pub agent_id: Option<String>,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorGenerateImageRequest {
    pub tool_call_id: String,
    pub description: String,
    pub file_path: Option<String>,
    #[serde(default)]
    pub reference_image_paths: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_plan_response_serializes_cursor_shape() {
        let response = CursorCreatePlanResponse {
            outcome: CursorCreatePlanOutcome::Accepted {
                plan_uri: Some("file:///tmp/plan.md".into()),
            },
        };
        let value = serde_json::to_value(response).unwrap();
        assert_eq!(value["outcome"]["outcome"], "accepted");
        assert_eq!(value["outcome"]["planUri"], "file:///tmp/plan.md");

        let cancelled = CursorCreatePlanResponse {
            outcome: CursorCreatePlanOutcome::Cancelled,
        };
        let value = serde_json::to_value(cancelled).unwrap();
        assert_eq!(value["outcome"]["outcome"], "cancelled");
    }
}
