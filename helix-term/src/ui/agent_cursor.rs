//! Interactive UI for Cursor ACP extension methods.

use helix_acp::{
    CursorAskQuestionOutcome, CursorAskQuestionRequest, CursorAskQuestionResponse,
    CursorCreatePlanOutcome, CursorCreatePlanRequest, CursorCreatePlanResponse,
    METHOD_ASK_QUESTION, METHOD_CREATE_PLAN,
};
use helix_view::{
    agent::{AgentBlockKind, AgentModeMeta, AgentQuestionOption},
    Editor,
};
use serde_json::{json, Value};
use tui::text::Span;
use tui::widgets::Row;

use crate::agent;
use crate::compositor::Compositor;
use crate::ui::{overlay::overlaid, Markdown, Picker, PickerColumn, Popup, Select};
use crate::ui::prompt::PromptEvent;

use super::menu::Item;

pub fn show_mode_picker(editor: &mut Editor, compositor: &mut Compositor) {
    let modes = editor.agent.available_modes.clone();
    if modes.is_empty() {
        editor.set_error("no agent modes available");
        return;
    }

    let columns = [PickerColumn::new("mode", |item: &AgentModeMeta, _| {
        item.name.as_str().into()
    })];

    let picker = Picker::new(columns, 0, modes, (), move |cx, mode, _action| {
        agent::with_controller(|controller| {
            controller.send(helix_acp::AgentCommand::SetMode {
                mode_id: mode.id.clone(),
            });
        });
        cx.editor.agent.mode = Some(mode.id.clone());
        cx.editor.set_status(format!("agent mode: {}", mode.name));
    })
    .truncate_start(false);

    compositor.push(Box::new(overlaid(picker)));
}

pub fn show_cursor_request_ui(editor: &mut Editor, compositor: &mut Compositor) {
    let Some(request) = editor.agent.cursor_request.take() else {
        return;
    };

    match request.method.as_str() {
        METHOD_ASK_QUESTION => show_ask_question(editor, compositor, request.request_id, request.params),
        METHOD_CREATE_PLAN => show_create_plan(editor, compositor, request.request_id, request.params),
        other => {
            editor.set_error(format!("unsupported cursor request: {other}"));
            respond_cursor(request.request_id, json!({ "outcome": { "outcome": "cancelled" } }));
        }
    }
}

fn show_ask_question(
    editor: &mut Editor,
    compositor: &mut Compositor,
    request_id: u64,
    params: Value,
) {
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

    show_next_question(editor, compositor);
}

fn show_next_question(editor: &mut Editor, compositor: &mut Compositor) {
    let Some(flow) = editor.agent.cursor_question_flow.as_ref() else {
        return;
    };
    let Some(question) = flow.questions.get(flow.index) else {
        finish_question_flow(editor);
        return;
    };

    let title = flow
        .title
        .clone()
        .unwrap_or_else(|| "Agent question".into());
    let prompt = question.prompt.clone();
    let options: Vec<QuestionPickerItem> = question
        .options
        .iter()
        .map(|option| QuestionPickerItem {
            id: option.id.clone(),
            label: option.label.clone(),
        })
        .collect();

    if options.is_empty() {
        editor.set_error("cursor question has no options");
        cancel_question_flow(editor);
        return;
    }

    let select = Select::new(
        format!("{title}\n\n{prompt}"),
        options,
        (),
        move |editor, option, event| match event {
            PromptEvent::Validate => {
                let Some(flow) = editor.agent.cursor_question_flow.as_mut() else {
                    return;
                };
                flow.answers.push(helix_view::agent::AgentQuestionAnswer {
                    question_id: flow.questions[flow.index].id.clone(),
                    selected_option_ids: vec![option.id.clone()],
                });
                flow.index += 1;
                if flow.index >= flow.questions.len() {
                    finish_question_flow(editor);
                } else {
                    editor.agent.open_cursor_request = true;
                }
            }
            PromptEvent::Abort => cancel_question_flow(editor),
            PromptEvent::Update => {}
        },
    );

    compositor.replace_or_push("cursor-ask-question", overlaid(select));
}

fn finish_question_flow(editor: &mut Editor) {
    let Some(flow) = editor.agent.cursor_question_flow.take() else {
        return;
    };
    let response = CursorAskQuestionResponse {
        outcome: CursorAskQuestionOutcome::Answered {
            answers: flow
                .answers
                .into_iter()
                .map(|answer| helix_acp::CursorQuestionAnswer {
                    question_id: answer.question_id,
                    selected_option_ids: answer.selected_option_ids,
                })
                .collect(),
        },
    };
    respond_cursor(
        flow.request_id,
        serde_json::to_value(response).unwrap_or_else(|_| {
            json!({ "outcome": { "outcome": "cancelled" } })
        }),
    );
}

fn cancel_question_flow(editor: &mut Editor) {
    let Some(flow) = editor.agent.cursor_question_flow.take() else {
        return;
    };
    respond_cursor(
        flow.request_id,
        serde_json::to_value(CursorAskQuestionResponse {
            outcome: CursorAskQuestionOutcome::Cancelled,
        })
        .unwrap_or_else(|_| json!({ "outcome": { "outcome": "cancelled" } })),
    );
}

fn show_create_plan(
    editor: &mut Editor,
    compositor: &mut Compositor,
    request_id: u64,
    params: Value,
) {
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

    let plan_name_for_accept = plan_name.clone();
    let markdown = Markdown::new(body, editor.syn_loader.clone());
    compositor.push(Box::new(overlaid(
        Popup::new("agent-plan-review", markdown).auto_close(true),
    )));

    let request_id_copy = request_id;
    let select = Select::new(
        "Review agent plan",
        vec![
            PlanChoice {
                label: "Accept plan",
                outcome: CursorCreatePlanOutcome::Accepted { plan_uri: None },
            },
            PlanChoice {
                label: "Reject plan",
                outcome: CursorCreatePlanOutcome::Rejected {
                    reason: Some("rejected by user".into()),
                },
            },
            PlanChoice {
                label: "Cancel",
                outcome: CursorCreatePlanOutcome::Cancelled,
            },
        ],
        (),
        move |editor, choice, event| match event {
            PromptEvent::Validate => {
                let response = CursorCreatePlanResponse {
                    outcome: choice.outcome.clone(),
                };
                respond_cursor(
                    request_id_copy,
                    serde_json::to_value(response).unwrap_or_else(|_| {
                        json!({ "outcome": { "outcome": "cancelled" } })
                    }),
                );
                if matches!(choice.outcome, CursorCreatePlanOutcome::Accepted { .. }) {
                    let label = plan_name_for_accept
                        .as_ref()
                        .map(|name| format!("Plan accepted: {name}"))
                        .unwrap_or_else(|| "Plan accepted".into());
                    editor.agent.push_block(AgentBlockKind::System { text: label });
                }
                editor.set_status("agent plan reviewed");
            }
            PromptEvent::Abort => {
                respond_cursor(
                    request_id_copy,
                    serde_json::to_value(CursorCreatePlanResponse {
                        outcome: CursorCreatePlanOutcome::Cancelled,
                    })
                    .unwrap_or_else(|_| json!({ "outcome": { "outcome": "cancelled" } })),
                );
            }
            PromptEvent::Update => {}
        },
    );

    compositor.push(Box::new(overlaid(select)));
}

fn respond_cursor(request_id: u64, result: Value) {
    agent::with_controller(|controller| {
        controller.respond_cursor(request_id, result);
    });
}

#[derive(Clone)]
struct QuestionPickerItem {
    id: String,
    label: String,
}

impl Item for QuestionPickerItem {
    type Data = ();

    fn format(&self, _data: &Self::Data) -> Row<'_> {
        Row::new(vec![Span::raw(self.label.clone())])
    }
}

#[derive(Clone)]
struct PlanChoice {
    label: &'static str,
    outcome: CursorCreatePlanOutcome,
}

impl Item for PlanChoice {
    type Data = ();

    fn format(&self, _data: &Self::Data) -> Row<'_> {
        Row::new(vec![Span::raw(self.label)])
    }
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
    if let Some(flow) = editor.agent.cursor_question_flow.take() {
        respond_cursor(
            flow.request_id,
            serde_json::to_value(CursorAskQuestionResponse {
                outcome: CursorAskQuestionOutcome::Cancelled,
            })
            .unwrap_or_else(|_| json!({ "outcome": { "outcome": "cancelled" } })),
        );
    }
    if let Some(request) = editor.agent.cursor_request.take() {
        respond_cursor(
            request.request_id,
            json!({ "outcome": { "outcome": "cancelled" } }),
        );
    }
    editor.agent.open_cursor_request = false;
}

pub fn resume_question_flow(editor: &mut Editor, compositor: &mut Compositor) {
    if editor.agent.cursor_question_flow.is_some() {
        show_next_question(editor, compositor);
    }
}
