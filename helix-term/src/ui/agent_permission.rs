//! Interactive UI for ACP permission requests.

use helix_view::{agent::AgentPermissionOption, Editor};
use tui::text::Span;
use tui::widgets::Row;

use crate::agent;
use crate::compositor::Compositor;
use crate::handlers::agent::{deny_tool_permission, grant_tool_permission};
use crate::ui::{menu::Item, overlay::overlaid, prompt::PromptEvent, Select};

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

pub fn show_permission_picker(editor: &mut Editor, compositor: &mut Compositor) {
    let Some(request) = editor.agent.pending_permission.clone() else {
        return;
    };

    if request.options.is_empty() {
        editor.agent.pending_permission = None;
        respond_permission(request.request_id, None);
        editor.set_error("permission request had no options");
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
        }
        PromptEvent::Abort => {
            deny_tool_permission(editor, tool_call_id.as_deref());
            editor.agent.pending_permission = None;
            respond_permission(request_id, None);
            editor.set_status("permission request cancelled");
        }
        _ => {}
    });

    compositor.push(Box::new(overlaid(select)));
}

fn respond_permission(request_id: u64, option_id: Option<String>) {
    agent::with_controller(|controller| {
        controller.send(helix_acp::AgentCommand::RespondPermission {
            request_id,
            option_id,
        });
    });
}

pub fn cancel_pending_permission(editor: &mut Editor) {
    let Some(request) = editor.agent.pending_permission.take() else {
        return;
    };
    deny_tool_permission(editor, request.tool_call_id.as_deref());
    respond_permission(request.request_id, None);
    editor.agent.open_permission_picker = false;
}
