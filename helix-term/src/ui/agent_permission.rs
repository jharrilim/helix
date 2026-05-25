//! Interactive UI for ACP permission requests.

use helix_core::Position;
use helix_view::{
    agent::{AgentFocus, AgentPermissionOption},
    graphics::{CursorKind, Rect},
    Editor,
};
use tui::buffer::Buffer as Surface;
use tui::text::Span;
use tui::widgets::Row;

use crate::agent;
use crate::compositor::{Component, Compositor, Context, Event, EventResult};
use crate::handlers::agent::{deny_tool_permission, grant_tool_permission};
use crate::ui::{menu::Item, overlay::overlaid, prompt::PromptEvent, Select};

const PERMISSION_PICKER_ID: &str = "agent-permission";

#[derive(Clone)]
struct PermissionPickerItem {
    id: String,
    label: String,
}

impl Item for PermissionPickerItem {
    type Data = ();

    fn format(&self, _data: &Self::Data) -> Row<'_> {
        Row::new(vec![Span::raw(self.label.clone())])
    }
}

struct PermissionPicker {
    select: crate::ui::overlay::Overlay<Select<PermissionPickerItem>>,
}

impl PermissionPicker {
    fn new(select: Select<PermissionPickerItem>) -> Self {
        Self {
            select: overlaid(select),
        }
    }
}

impl Component for PermissionPicker {
    fn handle_event(&mut self, event: &Event, ctx: &mut Context) -> EventResult {
        self.select.handle_event(event, ctx)
    }

    fn required_size(&mut self, viewport: (u16, u16)) -> Option<(u16, u16)> {
        self.select.required_size(viewport)
    }

    fn render(&mut self, area: Rect, surface: &mut Surface, ctx: &mut Context) {
        self.select.render(area, surface, ctx);
    }

    fn cursor(&self, area: Rect, ctx: &Editor) -> (Option<Position>, CursorKind) {
        self.select.cursor(area, ctx)
    }

    fn id(&self) -> Option<&'static str> {
        Some(PERMISSION_PICKER_ID)
    }
}

pub fn show_permission_picker(editor: &mut Editor, compositor: &mut Compositor) {
    let Some(request) = editor.agent.pending_permission.clone() else {
        return;
    };

    if request.options.is_empty() {
        editor.agent.pending_permission = None;
        respond_permission(request.request_id, None);
        editor.set_error("permission request had no options");
        restore_agent_input_focus(editor);
        compositor.remove(PERMISSION_PICKER_ID);
        return;
    }

    let title = request.title;
    let message = request.message;
    let request_id = request.request_id;
    let tool_call_id = request.tool_call_id;
    let options: Vec<PermissionPickerItem> = request
        .options
        .into_iter()
        .map(|option: AgentPermissionOption| PermissionPickerItem {
            id: option.id,
            label: option.label,
        })
        .collect();

    let prompt = if message.is_empty() {
        title
    } else {
        format!("{title}\n\n{message}")
    };

    let select = Select::new(prompt, options, (), move |editor, option, event| match event {
        PromptEvent::Validate => {
            grant_tool_permission(editor, tool_call_id.as_deref());
            editor.agent.pending_permission = None;
            respond_permission(request_id, Some(option.id.clone()));
            editor.set_status(format!("permission: {}", option.label));
            restore_agent_input_focus(editor);
        }
        PromptEvent::Abort => {
            deny_tool_permission(editor, tool_call_id.as_deref());
            editor.agent.pending_permission = None;
            respond_permission(request_id, None);
            editor.set_status("permission request cancelled");
            restore_agent_input_focus(editor);
        }
        _ => {}
    });

    compositor.replace_or_push(PERMISSION_PICKER_ID, PermissionPicker::new(select));
}

fn restore_agent_input_focus(editor: &mut Editor) {
    if editor
        .agent
        .panel_id
        .is_some_and(|panel_id| editor.tree.focus == panel_id)
    {
        editor.agent.focus = AgentFocus::Insert;
    }
    helix_event::request_redraw();
}

fn respond_permission(request_id: u64, option_id: Option<String>) {
    agent::with_controller(|controller| {
        controller.respond_permission(request_id, option_id);
    });
}

pub fn cancel_pending_permission(editor: &mut Editor) {
    let Some(request) = editor.agent.pending_permission.take() else {
        return;
    };
    deny_tool_permission(editor, request.tool_call_id.as_deref());
    respond_permission(request.request_id, None);
    editor.agent.open_permission_picker = false;
    restore_agent_input_focus(editor);
}

#[cfg(feature = "integration")]
pub fn restore_input_focus_after_permission(editor: &mut Editor) {
    restore_agent_input_focus(editor);
}

pub fn cancel_superseded_permission(editor: &mut Editor, request_id: u64, tool_call_id: Option<&str>) {
    deny_tool_permission(editor, tool_call_id);
    respond_permission(request_id, None);
}
