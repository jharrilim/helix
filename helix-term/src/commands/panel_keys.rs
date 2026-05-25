//! Mappable commands for auxiliary panel keymaps (git, agent, terminal).

use super::Context;

// --- Git panel ---

pub fn git_panel_move_down(cx: &mut Context) {
    crate::ui::move_selection_next(cx.editor);
}

pub fn git_panel_move_up(cx: &mut Context) {
    crate::ui::move_selection_prev(cx.editor);
}

pub fn git_panel_close(cx: &mut Context) {
    cx.editor.close_git_panel();
}

// --- Agent panel ---

pub fn agent_panel_insert(cx: &mut Context) {
    crate::ui::agent_enter_insert_mode(cx.editor);
}

pub fn agent_panel_toggle_collapsible(cx: &mut Context) {
    crate::ui::agent_toggle_collapsible_block(cx.editor);
}

pub fn agent_panel_page_up(cx: &mut Context) {
    cx.editor.agent.scroll = cx.editor.agent.scroll.saturating_add(1);
    helix_event::request_redraw();
}

pub fn agent_panel_page_down(cx: &mut Context) {
    cx.editor.agent.scroll = cx.editor.agent.scroll.saturating_sub(1);
    helix_event::request_redraw();
}

pub fn agent_panel_close(cx: &mut Context) {
    crate::commands::agent::close_agent_panel_editor(cx.editor);
}

// --- Terminal panel ---

pub fn terminal_panel_insert(cx: &mut Context) {
    crate::ui::terminal_enter_insert_mode(cx.editor);
}

pub fn terminal_panel_scroll_down(cx: &mut Context) {
    crate::ui::terminal_scroll_lines_by(cx.editor, -1);
}

pub fn terminal_panel_scroll_up(cx: &mut Context) {
    crate::ui::terminal_scroll_lines_by(cx.editor, 1);
}

pub fn terminal_panel_scroll_top(cx: &mut Context) {
    crate::ui::terminal_scroll_to_top(cx.editor);
}

pub fn terminal_panel_scroll_bottom(cx: &mut Context) {
    crate::ui::terminal_scroll_to_bottom(cx.editor);
}

pub fn terminal_panel_search(cx: &mut Context) {
    crate::ui::terminal::open_search_prompt(cx);
}

pub fn terminal_panel_tab_menu(cx: &mut Context) {
    if cx.editor.terminal.tab_menu_active {
        crate::ui::terminal_tabs::close(cx.editor);
    } else {
        crate::ui::terminal_tabs::open(cx.editor);
    }
}

pub fn terminal_panel_close(cx: &mut Context) {
    crate::commands::terminal::close_terminal_panel_editor(cx.editor);
}
