//! Interactive UI for Cursor ACP extension methods.

use helix_acp::{
    CursorAskQuestionOutcome, CursorAskQuestionRequest, CursorAskQuestionResponse,
    CursorCreatePlanRequest, METHOD_ASK_QUESTION, METHOD_CREATE_PLAN,
};
use helix_view::{
    agent::AgentQuestionOption,
    plan::{PlanFooterFocus, PlanReview},
    Editor,
};
use serde_json::{json, Value};

use crate::ui::plan;

pub fn show_cursor_request_ui(editor: &mut Editor) {
    let Some(request) = editor.agent.cursor_request.take() else {
        return;
    };

    match request.method.as_str() {
        METHOD_ASK_QUESTION => show_ask_question(editor, request.request_id, request.params),
        METHOD_CREATE_PLAN => show_create_plan(editor, request.request_id, request.params),
        other => {
            editor.set_error(format!("unsupported cursor request: {other}"));
            respond_cursor(request.request_id, json!({ "outcome": { "outcome": "cancelled" } }));
        }
    }
}

fn show_ask_question(editor: &mut Editor, request_id: u64, params: Value) {
    let Ok(request) = serde_json::from_value::<CursorAskQuestionRequest>(params) else {
        editor.set_error("failed to parse cursor/ask_question request");
        respond_cursor(request_id, json!({ "outcome": { "outcome": "cancelled" } }));
        return;
    };

    if request.questions.is_empty() {
        respond_cursor(
            request_id,
            serde_json::to_value(CursorAskQuestionResponse {
                outcome: CursorAskQuestionOutcome::Skipped {
                    reason: Some("no questions".into()),
                },
            })
            .unwrap(),
        );
        return;
    }

    editor.agent.cursor_question_flow = Some(helix_view::agent::AgentQuestionFlow {
        request_id,
        title: request.title,
        questions: request
            .questions
            .into_iter()
            .map(|question| helix_view::agent::AgentQuestion {
                id: question.id,
                prompt: question.prompt,
                options: question
                    .options
                    .into_iter()
                    .map(|option| AgentQuestionOption {
                        id: option.id,
                        label: option.label,
                    })
                    .collect(),
                allow_multiple: question.allow_multiple,
            })
            .collect(),
        answers: Vec::new(),
        index: 0,
    });

    if editor.plan.title.is_none() && editor.plan.review.is_none() {
        editor.plan.title = Some("Agent questions".into());
    }
    editor.plan.footer_focus = PlanFooterFocus::QuestionOption(0);
    editor.open_plan_panel();
    helix_event::request_redraw();
}

fn show_create_plan(editor: &mut Editor, request_id: u64, params: Value) {
    let Ok(request) = serde_json::from_value::<CursorCreatePlanRequest>(params) else {
        editor.set_error("failed to parse cursor/create_plan request");
        respond_cursor(request_id, json!({ "outcome": { "outcome": "cancelled" } }));
        return;
    };

    let plan_name = request.name.clone();
    let mut body = String::new();
    if let Some(name) = &plan_name {
        body.push_str(&format!("# {name}\n\n"));
    }
    if let Some(overview) = request.overview {
        body.push_str(&format!("{overview}\n\n"));
    }
    body.push_str(&request.plan);
    if !request.todos.is_empty() {
        body.push_str("\n\n## Todos\n");
        for todo in &request.todos {
            body.push_str(&format!("\n- [{}] {}", todo.status_label(), todo.content));
        }
    }

    editor.plan.review = Some(PlanReview {
        request_id,
        tool_call_id: request.tool_call_id,
        name: plan_name,
        markdown: body,
    });
    editor.plan.footer_focus = PlanFooterFocus::Accept;
    editor.plan.scroll = 0;
    editor.open_plan_panel();
    helix_event::request_redraw();
}

fn respond_cursor(request_id: u64, result: Value) {
    crate::agent::with_controller(|controller| {
        controller.respond_cursor(request_id, result);
    });
}

trait CursorTodoStatusLabel {
    fn status_label(&self) -> &'static str;
}

impl CursorTodoStatusLabel for helix_acp::CursorTodo {
    fn status_label(&self) -> &'static str {
        match self.status {
            helix_acp::CursorTodoStatus::Pending => "pending",
            helix_acp::CursorTodoStatus::InProgress => "in_progress",
            helix_acp::CursorTodoStatus::Completed => "completed",
            helix_acp::CursorTodoStatus::Cancelled => "cancelled",
        }
    }
}

pub fn cancel_pending_cursor_requests(editor: &mut Editor) {
    if editor.agent.cursor_question_flow.is_some() {
        plan::cancel_question_flow(editor);
    } else if editor.plan.review.is_some() {
        let request_id = editor.plan.review.as_ref().map(|review| review.request_id);
        editor.plan.review = None;
        if let Some(request_id) = request_id {
            respond_cursor(
                request_id,
                json!({ "outcome": { "outcome": "cancelled" } }),
            );
        }
    }
    if let Some(request) = editor.agent.cursor_request.take() {
        respond_cursor(
            request.request_id,
            json!({ "outcome": { "outcome": "cancelled" } }),
        );
    }
    editor.agent.open_cursor_request = false;
    editor.close_plan_panel();
}

pub fn resume_question_flow(editor: &mut Editor) {
    if editor.agent.cursor_question_flow.is_some() {
        editor.plan.footer_focus = PlanFooterFocus::QuestionOption(0);
        if !editor.plan.is_open() {
            editor.open_plan_panel();
        }
        helix_event::request_redraw();
    }
}
