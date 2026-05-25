//! Plan review panel rendering and input handling.

use helix_acp::{
    CursorAskQuestionOutcome, CursorAskQuestionResponse, CursorCreatePlanOutcome,
    CursorCreatePlanResponse,
};
use helix_core::unicode::segmentation::UnicodeSegmentation;
use helix_core::unicode::width::UnicodeWidthStr;
use helix_view::{
    agent::AgentBlockKind,
    graphics::{Modifier, Rect},
    input::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind},
    plan::PlanFooterFocus,
    Editor, ViewId,
};
use serde_json::json;
use tui::buffer::Buffer as Surface;
use tui::text::Span;
use tui::widgets::{Block, Widget};

use crate::agent;
use crate::compositor::EventResult;
use crate::ui::Markdown;

const ACTION_BAR_HEIGHT: u16 = 1;
const QUESTION_FOOTER_HEIGHT: u16 = 2;

#[derive(Clone)]
struct PlanLine {
    text: String,
    style: helix_view::graphics::Style,
}

struct PlanLayout {
    body_area: Rect,
    question_area: Rect,
    action_area: Rect,
    lines: Vec<PlanLine>,
}

pub fn panel_at_coords(editor: &Editor, row: u16, column: u16) -> Option<ViewId> {
    editor.tree.plan_panels().find_map(|(panel, _)| {
        contains_coords(panel.area, row, column).then_some(panel.id)
    })
}

fn contains_coords(area: Rect, row: u16, column: u16) -> bool {
    row >= area.y && row < area.bottom() && column >= area.x && column < area.right()
}

fn panel_inner(area: Rect) -> Rect {
    super::panel_style::panel_inner(area)
}

fn build_layout(editor: &Editor, area: Rect) -> PlanLayout {
    let inner = panel_inner(area);
    let has_questions = editor.agent.cursor_question_flow.is_some();
    let has_review = editor.plan.review.is_some();

    let action_height = if has_review {
        ACTION_BAR_HEIGHT.min(inner.height)
    } else {
        0
    };
    let question_height = if has_questions {
        QUESTION_FOOTER_HEIGHT.min(inner.height.saturating_sub(action_height))
    } else {
        0
    };

    let action_area = if action_height > 0 {
        Rect {
            y: inner.bottom().saturating_sub(action_height),
            height: action_height,
            ..inner
        }
    } else {
        Rect::default()
    };

    let question_area = if question_height > 0 {
        Rect {
            y: action_area.y.saturating_sub(question_height),
            height: question_height,
            ..inner
        }
    } else {
        Rect::default()
    };

    let body_bottom = if question_height > 0 {
        question_area.y
    } else if action_height > 0 {
        action_area.y
    } else {
        inner.bottom()
    };
    let body_area = Rect {
        y: inner.y,
        height: body_bottom.saturating_sub(inner.y),
        ..inner
    };

    let width = body_area.width.saturating_sub(2) as usize;
    let markdown = plan_markdown(editor);
    let lines = render_markdown_lines(editor, &markdown, width);

    PlanLayout {
        body_area,
        question_area,
        action_area,
        lines,
    }
}

fn plan_markdown(editor: &Editor) -> String {
    if let Some(review) = &editor.plan.review {
        return review.markdown.clone();
    }
    editor
        .plan
        .title
        .clone()
        .map(|title| format!("# {title}\n"))
        .unwrap_or_else(|| "# Agent questions\n".into())
}

fn render_markdown_lines(editor: &Editor, text: &str, width: usize) -> Vec<PlanLine> {
    let theme = &editor.theme;
    let text_style = theme.get("ui.text");
    let markdown = Markdown::new(text.to_string(), editor.syn_loader.clone());
    let rendered = markdown.parse(Some(theme));
    let mut lines = Vec::new();

    for line in rendered.lines {
        if line.0.is_empty() {
            lines.push(PlanLine {
                text: String::new(),
                style: text_style,
            });
            continue;
        }
        let plain: String = line.0.iter().map(|span| span.content.as_ref()).collect();
        if width == 0 || plain.width() <= width {
            lines.push(PlanLine {
                text: plain,
                style: text_style,
            });
            continue;
        }
        let mut current = String::new();
        let mut current_width = 0usize;
        for span in line.0 {
            for grapheme in span.content.graphemes(true) {
                let grapheme_width = grapheme.width();
                if current_width > 0 && current_width + grapheme_width > width {
                    lines.push(PlanLine {
                        text: current.clone(),
                        style: text_style,
                    });
                    current.clear();
                    current_width = 0;
                }
                current.push_str(grapheme);
                current_width += grapheme_width;
            }
        }
        if !current.is_empty() {
            lines.push(PlanLine {
                text: current,
                style: text_style,
            });
        }
    }

    if lines.is_empty() {
        lines.push(PlanLine {
            text: String::new(),
            style: text_style,
        });
    }
    lines
}

pub fn render(editor: &Editor, area: Rect, surface: &mut Surface, focused: bool) {
    let theme = &editor.theme;
    let border_style = super::panel_style::border_style(theme);
    let label_style = theme.get("ui.text");
    let prompt_style = theme.get("ui.help");
    let action_style = theme.get("ui.text");
    let action_focus_style = theme.get("ui.selection");
    let status_style = super::panel_style::statusline_style(theme, focused);

    let title = editor
        .plan
        .review
        .as_ref()
        .and_then(|review| review.name.clone())
        .map(|name| format!(" Plan: {name} "))
        .or_else(|| editor.plan.title.clone().map(|title| format!(" {title} ")))
        .unwrap_or_else(|| " Plan review ".into());

    let block = Block::default()
        .borders(super::panel_style::panel_borders())
        .border_style(border_style)
        .title(Span::styled(title, label_style.add_modifier(Modifier::BOLD)));
    block.render(area, surface);

    let layout = build_layout(editor, area);
    let visible = layout.body_area.height as usize;
    let scroll = editor.plan.scroll.min(layout.lines.len().saturating_sub(1));

    for (row, line) in layout
        .lines
        .iter()
        .skip(scroll)
        .take(visible)
        .enumerate()
    {
        let y = layout.body_area.y + row as u16;
        surface.set_stringn(
            layout.body_area.x + 1,
            y,
            &line.text,
            layout.body_area.width.saturating_sub(2) as usize,
            line.style,
        );
    }

    if layout.question_area.height > 0 {
        render_question_footer(editor, layout.question_area, surface, prompt_style, action_focus_style);
    }

    if layout.action_area.height > 0 {
        render_action_bar(editor, layout.action_area, surface, action_style, action_focus_style, status_style);
    }
}

fn render_question_footer(
    editor: &Editor,
    area: Rect,
    surface: &mut Surface,
    prompt_style: helix_view::graphics::Style,
    focus_style: helix_view::graphics::Style,
) {
    let Some(flow) = editor.agent.cursor_question_flow.as_ref() else {
        return;
    };
    let Some(question) = flow.questions.get(flow.index) else {
        return;
    };

    let title = flow
        .title
        .as_deref()
        .unwrap_or("Agent question");
    let header = format!("{title}: {}", question.prompt);
    surface.set_stringn(
        area.x + 1,
        area.y,
        &header,
        area.width.saturating_sub(2) as usize,
        prompt_style,
    );

    let mut x = area.x + 1;
    let y = area.y + 1;
    for (index, option) in question.options.iter().enumerate().take(9) {
        if y >= area.bottom() {
            break;
        }
        let focused = matches!(
            editor.plan.footer_focus,
            PlanFooterFocus::QuestionOption(i) if i == index
        );
        let label = format!(" {}:{} {} ", index + 1, if focused { ">" } else { " " }, option.label);
        let style = if focused { focus_style } else { prompt_style };
        surface.set_stringn(x, y, &label, label.width(), style);
        x = x.saturating_add(label.width() as u16 + 1);
    }
}

fn render_action_bar(
    editor: &Editor,
    area: Rect,
    surface: &mut Surface,
    action_style: helix_view::graphics::Style,
    focus_style: helix_view::graphics::Style,
    status_style: helix_view::graphics::Style,
) {
    surface.set_style(area, status_style);
    let clear_width = area.width as usize;
    if clear_width > 0 {
        surface.set_stringn(
            area.x,
            area.y,
            &" ".repeat(clear_width),
            clear_width,
            action_style,
        );
    }
    let actions = [
        (PlanFooterFocus::Accept, "Accept"),
        (PlanFooterFocus::Reject, "Reject"),
        (PlanFooterFocus::Cancel, "Cancel"),
    ];
    let mut x = area.x + 1;
    for (index, (focus, label)) in actions.iter().enumerate() {
        let style = if editor.plan.footer_focus == *focus {
            focus_style
        } else {
            action_style
        };
        let text = format!("[{label}]");
        surface.set_stringn(x, area.y, &text, text.len(), style);
        x = x.saturating_add(text.width() as u16);
        if index + 1 < actions.len() {
            x = x.saturating_add(1);
        }
    }
}

pub fn handle_normal_key(editor: &mut Editor, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Char(c @ '1'..='9') if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT => {
            select_question_option(editor, (c as u8 - b'1') as usize);
            true
        }
        KeyCode::Left | KeyCode::Char('h') => {
            footer_focus_prev(editor);
            true
        }
        KeyCode::Right | KeyCode::Char('l') => {
            footer_focus_next(editor);
            true
        }
        KeyCode::Tab if key.modifiers.contains(KeyModifiers::SHIFT) => {
            footer_focus_prev(editor);
            true
        }
        KeyCode::Tab => {
            footer_focus_next(editor);
            true
        }
        KeyCode::Enter => {
            footer_activate(editor);
            true
        }
        KeyCode::Esc => {
            cancel_plan_flow(editor);
            true
        }
        _ => false,
    }
}

pub fn page_up(editor: &mut Editor) {
    editor.plan.scroll = editor.plan.scroll.saturating_add(1);
    helix_event::request_redraw();
}

pub fn page_down(editor: &mut Editor) {
    editor.plan.scroll = editor.plan.scroll.saturating_sub(1);
    helix_event::request_redraw();
}

fn footer_focus_next(editor: &mut Editor) {
    if editor.agent.cursor_question_flow.is_some() {
        if let Some(count) = current_question_option_count(editor) {
            editor.plan.footer_focus = match editor.plan.footer_focus {
                PlanFooterFocus::QuestionOption(index) if index + 1 < count => {
                    PlanFooterFocus::QuestionOption(index + 1)
                }
                PlanFooterFocus::QuestionOption(_) => PlanFooterFocus::Accept,
                PlanFooterFocus::Accept => PlanFooterFocus::Reject,
                PlanFooterFocus::Reject => PlanFooterFocus::Cancel,
                PlanFooterFocus::Cancel => PlanFooterFocus::QuestionOption(0),
            };
            helix_event::request_redraw();
            return;
        }
    }
    editor.plan.footer_focus = match editor.plan.footer_focus {
        PlanFooterFocus::Accept => PlanFooterFocus::Reject,
        PlanFooterFocus::Reject => PlanFooterFocus::Cancel,
        PlanFooterFocus::Cancel | PlanFooterFocus::QuestionOption(_) => PlanFooterFocus::Accept,
    };
    helix_event::request_redraw();
}

fn footer_focus_prev(editor: &mut Editor) {
    if editor.agent.cursor_question_flow.is_some() {
        if let Some(count) = current_question_option_count(editor) {
            editor.plan.footer_focus = match editor.plan.footer_focus {
                PlanFooterFocus::QuestionOption(index) if index > 0 => {
                    PlanFooterFocus::QuestionOption(index - 1)
                }
                PlanFooterFocus::QuestionOption(_) => PlanFooterFocus::Cancel,
                PlanFooterFocus::Cancel => PlanFooterFocus::Reject,
                PlanFooterFocus::Reject => PlanFooterFocus::Accept,
                PlanFooterFocus::Accept => PlanFooterFocus::QuestionOption(count.saturating_sub(1)),
            };
            helix_event::request_redraw();
            return;
        }
    }
    editor.plan.footer_focus = match editor.plan.footer_focus {
        PlanFooterFocus::Cancel => PlanFooterFocus::Reject,
        PlanFooterFocus::Reject => PlanFooterFocus::Accept,
        PlanFooterFocus::Accept | PlanFooterFocus::QuestionOption(_) => PlanFooterFocus::Cancel,
    };
    helix_event::request_redraw();
}

fn current_question_option_count(editor: &Editor) -> Option<usize> {
    let flow = editor.agent.cursor_question_flow.as_ref()?;
    Some(flow.questions.get(flow.index)?.options.len())
}

pub fn footer_activate(editor: &mut Editor) {
    match editor.plan.footer_focus {
        PlanFooterFocus::QuestionOption(index) => select_question_option(editor, index),
        PlanFooterFocus::Accept => accept_plan_review(editor),
        PlanFooterFocus::Reject => reject_plan_review(editor),
        PlanFooterFocus::Cancel => cancel_plan_review(editor),
    }
}

pub fn select_question_option(editor: &mut Editor, index: usize) {
    let Some(option_id) = editor
        .agent
        .cursor_question_flow
        .as_ref()
        .and_then(|flow| flow.questions.get(flow.index))
        .and_then(|question| question.options.get(index))
        .map(|option| option.id.clone())
    else {
        return;
    };

    let Some(flow) = editor.agent.cursor_question_flow.as_mut() else {
        return;
    };
    flow.answers.push(helix_view::agent::AgentQuestionAnswer {
        question_id: flow.questions[flow.index].id.clone(),
        selected_option_ids: vec![option_id],
    });
    flow.index += 1;
    if flow.index >= flow.questions.len() {
        finish_question_flow(editor);
    } else {
        editor.plan.footer_focus = PlanFooterFocus::QuestionOption(0);
        helix_event::request_redraw();
    }
}

pub fn finish_question_flow(editor: &mut Editor) {
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
    maybe_close_plan_panel(editor);
    editor.set_status("agent questions answered");
}

pub fn cancel_question_flow(editor: &mut Editor) {
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
    maybe_close_plan_panel(editor);
    editor.set_status("agent questions cancelled");
}

pub fn accept_plan_review(editor: &mut Editor) {
    let Some(review) = editor.plan.review.clone() else {
        return;
    };
    let plan_uri = crate::handlers::agent::write_accepted_plan(
        editor,
        review.name.as_deref(),
        &review.tool_call_id,
        &review.markdown,
    );
    if editor.agent.mode.as_deref() != Some("agent") {
        editor.agent.continue_after_plan_accept = true;
    }
    let response = CursorCreatePlanResponse {
        outcome: CursorCreatePlanOutcome::Accepted { plan_uri },
    };
    respond_cursor(
        review.request_id,
        serde_json::to_value(response).unwrap_or_else(|_| {
            json!({ "outcome": { "outcome": "cancelled" } })
        }),
    );
    let label = review
        .name
        .as_ref()
        .map(|name| format!("Plan accepted: {name}"))
        .unwrap_or_else(|| "Plan accepted".into());
    editor.agent.push_block(AgentBlockKind::System { text: label });
    editor.plan.review = None;
    maybe_close_plan_panel(editor);
    editor.set_status("agent plan accepted");
}

pub fn reject_plan_review(editor: &mut Editor) {
    let Some(review) = editor.plan.review.take() else {
        return;
    };
    respond_cursor(
        review.request_id,
        serde_json::to_value(CursorCreatePlanResponse {
            outcome: CursorCreatePlanOutcome::Rejected {
                reason: Some("rejected by user".into()),
            },
        })
        .unwrap_or_else(|_| json!({ "outcome": { "outcome": "cancelled" } })),
    );
    maybe_close_plan_panel(editor);
    editor.set_status("agent plan rejected");
}

pub fn cancel_plan_review(editor: &mut Editor) {
    let Some(review) = editor.plan.review.take() else {
        cancel_plan_flow(editor);
        return;
    };
    respond_cursor(
        review.request_id,
        serde_json::to_value(CursorCreatePlanResponse {
            outcome: CursorCreatePlanOutcome::Cancelled,
        })
        .unwrap_or_else(|_| json!({ "outcome": { "outcome": "cancelled" } })),
    );
    maybe_close_plan_panel(editor);
    editor.set_status("agent plan cancelled");
}

pub fn cancel_plan_flow(editor: &mut Editor) {
    if editor.agent.cursor_question_flow.is_some() {
        cancel_question_flow(editor);
        return;
    }
    if editor.plan.review.is_some() {
        cancel_plan_review(editor);
        return;
    }
    editor.close_plan_panel();
}

fn maybe_close_plan_panel(editor: &mut Editor) {
    if editor.plan.review.is_none() && editor.agent.cursor_question_flow.is_none() {
        editor.close_plan_panel();
    }
}

fn respond_cursor(request_id: u64, result: serde_json::Value) {
    agent::with_controller(|controller| {
        controller.respond_cursor(request_id, result);
    });
}

pub fn handle_mouse(editor: &mut Editor, event: MouseEvent) -> EventResult {
    if matches!(event.kind, MouseEventKind::Moved) {
        return EventResult::Ignored(None);
    }

    let panel_id = match panel_at_coords(editor, event.row, event.column) {
        Some(id) => id,
        None => return EventResult::Ignored(None),
    };
    editor.tree.focus = panel_id;

    if matches!(event.kind, MouseEventKind::ScrollUp | MouseEventKind::ScrollDown) {
        match event.kind {
            MouseEventKind::ScrollUp => page_up(editor),
            MouseEventKind::ScrollDown => page_down(editor),
            _ => {}
        }
        return EventResult::Consumed(None);
    }

    EventResult::Ignored(None)
}
