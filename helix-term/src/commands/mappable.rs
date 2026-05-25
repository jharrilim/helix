use std::fmt;

use anyhow::{anyhow, ensure};
use helix_core::command_line;
use helix_view::input::KeyEvent;
use serde::de::{self, Deserialize, Deserializer};

use crate::{compositor, ui::PromptEvent};

use super::{typed, Context, *};

/// MappableCommands are commands that can be bound to keys, executable in
/// normal, insert or select mode.
///
/// There are three kinds:
///
/// * Static: commands usually bound to keys and used for editing, movement,
///   etc., for example `move_char_left`.
/// * Typable: commands executable from command mode, prefixed with a `:`,
///   for example `:write!`.
/// * Macro: a sequence of keys to execute, for example `@miw`.
#[derive(Clone)]
pub enum MappableCommand {
    Typable {
        name: String,
        args: String,
        doc: String,
    },
    Static {
        name: &'static str,
        fun: fn(cx: &mut Context),
        doc: &'static str,
    },
    Macro {
        name: String,
        keys: Vec<KeyEvent>,
    },
}

macro_rules! static_commands {
    ( $($name:ident, $doc:literal,)* ) => {
        $(
            #[allow(non_upper_case_globals)]
            pub const $name: Self = Self::Static {
                name: stringify!($name),
                fun: $name,
                doc: $doc
            };
        )*

        pub const STATIC_COMMAND_LIST: &'static [Self] = &[
            $( Self::$name, )*
        ];
    }
}

impl MappableCommand {
    pub fn execute(&self, cx: &mut Context) {
        match &self {
            Self::Typable { name, args, doc: _ } => {
                if let Some(command) = typed::TYPABLE_COMMAND_MAP.get(name.as_str()) {
                    let mut cx = compositor::Context {
                        editor: cx.editor,
                        jobs: cx.jobs,
                        scroll: None,
                    };
                    if let Err(e) =
                        typed::execute_command(&mut cx, command, args, PromptEvent::Validate)
                    {
                        cx.editor.set_error(format!("{}", e));
                    }
                } else {
                    cx.editor.set_error(format!("no such command: '{name}'"));
                }
            }
            Self::Static { fun, .. } => (fun)(cx),
            Self::Macro { keys, .. } => {
                // Protect against recursive macros.
                if cx.editor.macro_replaying.contains(&'@') {
                    cx.editor.set_error(
                        "Cannot execute macro because the [@] register is already playing a macro",
                    );
                    return;
                }
                cx.editor.macro_replaying.push('@');
                let keys = keys.clone();
                cx.callback.push(Box::new(move |compositor, cx| {
                    for key in keys.into_iter() {
                        compositor.handle_event(&compositor::Event::Key(key), cx);
                    }
                    cx.editor.macro_replaying.pop();
                }));
            }
        }
    }

    pub fn name(&self) -> &str {
        match &self {
            Self::Typable { name, .. } => name,
            Self::Static { name, .. } => name,
            Self::Macro { name, .. } => name,
        }
    }

    pub fn doc(&self) -> &str {
        match &self {
            Self::Typable { doc, .. } => doc,
            Self::Static { doc, .. } => doc,
            Self::Macro { name, .. } => name,
        }
    }

    #[rustfmt::skip]
    static_commands!(
        no_op, "Do nothing",
        move_char_left, "Move left",
        move_char_right, "Move right",
        move_line_up, "Move up",
        move_line_down, "Move down",
        move_visual_line_up, "Move up",
        move_visual_line_down, "Move down",
        extend_char_left, "Extend left",
        extend_char_right, "Extend right",
        extend_line_up, "Extend up",
        extend_line_down, "Extend down",
        extend_visual_line_up, "Extend up",
        extend_visual_line_down, "Extend down",
        copy_selection_on_next_line, "Copy selection on next line",
        copy_selection_on_prev_line, "Copy selection on previous line",
        move_next_word_start, "Move to start of next word",
        move_prev_word_start, "Move to start of previous word",
        move_next_word_end, "Move to end of next word",
        move_prev_word_end, "Move to end of previous word",
        move_next_long_word_start, "Move to start of next long word",
        move_prev_long_word_start, "Move to start of previous long word",
        move_next_long_word_end, "Move to end of next long word",
        move_prev_long_word_end, "Move to end of previous long word",
        move_next_sub_word_start, "Move to start of next sub word",
        move_prev_sub_word_start, "Move to start of previous sub word",
        move_next_sub_word_end, "Move to end of next sub word",
        move_prev_sub_word_end, "Move to end of previous sub word",
        move_parent_node_end, "Move to end of the parent node",
        move_parent_node_start, "Move to beginning of the parent node",
        extend_next_word_start, "Extend to start of next word",
        extend_prev_word_start, "Extend to start of previous word",
        extend_next_word_end, "Extend to end of next word",
        extend_prev_word_end, "Extend to end of previous word",
        extend_next_long_word_start, "Extend to start of next long word",
        extend_prev_long_word_start, "Extend to start of previous long word",
        extend_next_long_word_end, "Extend to end of next long word",
        extend_prev_long_word_end, "Extend to end of prev long word",
        extend_next_sub_word_start, "Extend to start of next sub word",
        extend_prev_sub_word_start, "Extend to start of previous sub word",
        extend_next_sub_word_end, "Extend to end of next sub word",
        extend_prev_sub_word_end, "Extend to end of prev sub word",
        extend_parent_node_end, "Extend to end of the parent node",
        extend_parent_node_start, "Extend to beginning of the parent node",
        find_till_char, "Move till next occurrence of char",
        find_next_char, "Move to next occurrence of char",
        extend_till_char, "Extend till next occurrence of char",
        extend_next_char, "Extend to next occurrence of char",
        till_prev_char, "Move till previous occurrence of char",
        find_prev_char, "Move to previous occurrence of char",
        extend_till_prev_char, "Extend till previous occurrence of char",
        extend_prev_char, "Extend to previous occurrence of char",
        repeat_last_motion, "Repeat last motion",
        replace, "Replace with new char",
        switch_case, "Switch (toggle) case",
        switch_to_uppercase, "Switch to uppercase",
        switch_to_lowercase, "Switch to lowercase",
        page_up, "Move page up",
        page_down, "Move page down",
        half_page_up, "Move half page up",
        half_page_down, "Move half page down",
        page_cursor_up, "Move page and cursor up",
        page_cursor_down, "Move page and cursor down",
        page_cursor_half_up, "Move page and cursor half up",
        page_cursor_half_down, "Move page and cursor half down",
        select_all, "Select whole document",
        select_regex, "Select all regex matches inside selections",
        split_selection, "Split selections on regex matches",
        split_selection_on_newline, "Split selection on newlines",
        merge_selections, "Merge selections",
        merge_consecutive_selections, "Merge consecutive selections",
        search, "Search for regex pattern",
        rsearch, "Reverse search for regex pattern",
        search_next, "Select next search match",
        search_prev, "Select previous search match",
        extend_search_next, "Add next search match to selection",
        extend_search_prev, "Add previous search match to selection",
        search_selection, "Use current selection as search pattern",
        search_selection_detect_word_boundaries, "Use current selection as the search pattern, automatically wrapping with `\\b` on word boundaries",
        make_search_word_bounded, "Modify current search to make it word bounded",
        global_search, "Global search in workspace folder",
        extend_line, "Select current line, if already selected, extend to another line based on the anchor",
        extend_line_below, "Select current line, if already selected, extend to next line",
        extend_line_above, "Select current line, if already selected, extend to previous line",
        select_line_above, "Select current line, if already selected, extend or shrink line above based on the anchor",
        select_line_below, "Select current line, if already selected, extend or shrink line below based on the anchor",
        extend_to_line_bounds, "Extend selection to line bounds",
        shrink_to_line_bounds, "Shrink selection to line bounds",
        delete_selection, "Delete selection",
        delete_selection_noyank, "Delete selection without yanking",
        change_selection, "Change selection",
        change_selection_noyank, "Change selection without yanking",
        collapse_selection, "Collapse selection into single cursor",
        flip_selections, "Flip selection cursor and anchor",
        ensure_selections_forward, "Ensure all selections face forward",
        insert_mode, "Insert before selection",
        append_mode, "Append after selection",
        command_mode, "Enter command mode",
        file_picker, "Open file picker",
        file_picker_in_current_buffer_directory, "Open file picker at current buffer's directory",
        file_picker_in_current_directory, "Open file picker at current working directory",
        file_explorer, "Open file explorer in workspace root",
        file_explorer_in_current_buffer_directory, "Open file explorer at current buffer's directory",
        file_explorer_in_current_directory, "Open file explorer at current working directory",
        code_action, "Perform code action",
        buffer_picker, "Open buffer picker",
        jumplist_picker, "Open jumplist picker",
        symbol_picker, "Open symbol picker",
        syntax_symbol_picker, "Open symbol picker from syntax information",
        lsp_or_syntax_symbol_picker, "Open symbol picker from LSP or syntax information",
        changed_file_picker, "Open changed file picker",
        select_references_to_symbol_under_cursor, "Select symbol references",
        workspace_symbol_picker, "Open workspace symbol picker",
        syntax_workspace_symbol_picker, "Open workspace symbol picker from syntax information",
        lsp_or_syntax_workspace_symbol_picker, "Open workspace symbol picker from LSP or syntax information",
        diagnostics_picker, "Open diagnostic picker",
        workspace_diagnostics_picker, "Open workspace diagnostic picker",
        last_picker, "Open last picker",
        insert_at_line_start, "Insert at start of line",
        insert_at_line_end, "Insert at end of line",
        open_below, "Open new line below selection",
        open_above, "Open new line above selection",
        normal_mode, "Enter normal mode",
        select_mode, "Enter selection extend mode",
        exit_select_mode, "Exit selection mode",
        goto_definition, "Goto definition",
        goto_declaration, "Goto declaration",
        add_newline_above, "Add newline above",
        add_newline_below, "Add newline below",
        goto_type_definition, "Goto type definition",
        goto_implementation, "Goto implementation",
        goto_file_start, "Goto line number `<n>` else file start",
        goto_file_end, "Goto file end",
        extend_to_file_start, "Extend to line number `<n>` else file start",
        extend_to_file_end, "Extend to file end",
        goto_file, "Goto files/URLs in selections",
        goto_file_hsplit, "Goto files in selections (hsplit)",
        goto_file_vsplit, "Goto files in selections (vsplit)",
        goto_reference, "Goto references",
        goto_window_top, "Goto window top",
        goto_window_center, "Goto window center",
        goto_window_bottom, "Goto window bottom",
        goto_last_accessed_file, "Goto last accessed file",
        goto_last_modified_file, "Goto last modified file",
        goto_last_modification, "Goto last modification",
        goto_line, "Goto line",
        goto_last_line, "Goto last line",
        extend_to_last_line, "Extend to last line",
        goto_first_diag, "Goto first diagnostic",
        goto_last_diag, "Goto last diagnostic",
        goto_next_diag, "Goto next diagnostic",
        goto_prev_diag, "Goto previous diagnostic",
        goto_next_change, "Goto next change",
        goto_prev_change, "Goto previous change",
        goto_first_change, "Goto first change",
        goto_last_change, "Goto last change",
        goto_line_start, "Goto line start",
        goto_line_end, "Goto line end",
        goto_column, "Goto column",
        extend_to_column, "Extend to column",
        goto_next_buffer, "Goto next buffer",
        goto_previous_buffer, "Goto previous buffer",
        goto_line_end_newline, "Goto newline at line end",
        goto_first_nonwhitespace, "Goto first non-blank in line",
        trim_selections, "Trim whitespace from selections",
        extend_to_line_start, "Extend to line start",
        extend_to_first_nonwhitespace, "Extend to first non-blank in line",
        extend_to_line_end, "Extend to line end",
        extend_to_line_end_newline, "Extend to line end",
        signature_help, "Show signature help",
        smart_tab, "Insert tab if all cursors have all whitespace to their left; otherwise, run a separate command.",
        insert_tab, "Insert tab char",
        insert_newline, "Insert newline char",
        insert_char_interactive, "Insert an interactively-chosen char",
        append_char_interactive, "Append an interactively-chosen char",
        delete_char_backward, "Delete previous char",
        delete_char_forward, "Delete next char",
        delete_word_backward, "Delete previous word",
        delete_word_forward, "Delete next word",
        kill_to_line_start, "Delete till start of line",
        kill_to_line_end, "Delete till end of line",
        undo, "Undo change",
        redo, "Redo change",
        earlier, "Move backward in history",
        later, "Move forward in history",
        commit_undo_checkpoint, "Commit changes to new checkpoint",
        yank, "Yank selection",
        yank_to_clipboard, "Yank selections to clipboard",
        yank_to_primary_clipboard, "Yank selections to primary clipboard",
        yank_joined, "Join and yank selections",
        yank_joined_to_clipboard, "Join and yank selections to clipboard",
        yank_main_selection_to_clipboard, "Yank main selection to clipboard",
        yank_joined_to_primary_clipboard, "Join and yank selections to primary clipboard",
        yank_main_selection_to_primary_clipboard, "Yank main selection to primary clipboard",
        replace_with_yanked, "Replace with yanked text",
        replace_selections_with_clipboard, "Replace selections by clipboard content",
        replace_selections_with_primary_clipboard, "Replace selections by primary clipboard",
        paste_after, "Paste after selection",
        paste_before, "Paste before selection",
        paste_clipboard_after, "Paste clipboard after selections",
        paste_clipboard_before, "Paste clipboard before selections",
        paste_primary_clipboard_after, "Paste primary clipboard after selections",
        paste_primary_clipboard_before, "Paste primary clipboard before selections",
        indent, "Indent selection",
        unindent, "Unindent selection",
        format_selections, "Format selection",
        join_selections, "Join lines inside selection",
        join_selections_space, "Join lines inside selection and select spaces",
        keep_selections, "Keep selections matching regex",
        remove_selections, "Remove selections matching regex",
        align_selections, "Align selections in column",
        keep_primary_selection, "Keep primary selection",
        remove_primary_selection, "Remove primary selection",
        completion, "Invoke completion popup",
        hover, "Show docs for item under cursor",
        toggle_comments, "Comment/uncomment selections",
        toggle_line_comments, "Line comment/uncomment selections",
        toggle_block_comments, "Block comment/uncomment selections",
        rotate_selections_forward, "Rotate selections forward",
        rotate_selections_backward, "Rotate selections backward",
        rotate_selection_contents_forward, "Rotate selection contents forward",
        rotate_selection_contents_backward, "Rotate selections contents backward",
        reverse_selection_contents, "Reverse selections contents",
        expand_selection, "Expand selection to parent syntax node",
        shrink_selection, "Shrink selection to previously expanded syntax node",
        select_next_sibling, "Select next sibling in the syntax tree",
        select_prev_sibling, "Select previous sibling the in syntax tree",
        select_all_siblings, "Select all siblings of the current node",
        select_all_children, "Select all children of the current node",
        fold_toggle, "Toggle code fold at cursor",
        fold_open, "Open code fold at cursor",
        fold_close, "Close code fold at cursor",
        fold_open_all, "Open all code folds",
        fold_close_all, "Close all code folds",
        agent_open, "Open agent panel",
        agent_close, "Close agent panel",
        agent_focus, "Focus agent panel",
        agent_focus_editor, "Focus editor from agent panel",
        agent_send, "Send agent prompt",
        agent_stop, "Stop agent session",
        agent_clear, "Clear agent transcript",
        agent_history, "List and load agent sessions",
        agent_new, "Start a new agent session",
        agent_mode, "Set or pick agent session mode",
        git_panel_toggle, "Toggle git panel",
        git_open, "Open selected git file",
        git_diff, "Diff selected git file",
        git_stage_selected, "Stage selected git file",
        git_unstage_selected, "Unstage selected git file",
        git_toggle_stage_selected, "Stage or unstage selected git file",
        git_stage_all, "Stage all git changes",
        git_commit_prompt, "Commit staged git changes",
        git_refresh, "Refresh git panel",
        git_focus_editor, "Focus editor from git panel",
        git_panel_move_down, "Move git panel selection down",
        git_panel_move_up, "Move git panel selection up",
        git_panel_close, "Close git panel",
        agent_panel_insert, "Enter agent prompt insert mode",
        agent_panel_toggle_collapsible, "Toggle focused agent collapsible block",
        agent_panel_page_up, "Scroll agent transcript up",
        agent_panel_page_down, "Scroll agent transcript down",
        agent_panel_close, "Close agent panel",
        terminal_panel_insert, "Enter terminal insert mode",
        terminal_panel_scroll_down, "Scroll terminal output down",
        terminal_panel_scroll_up, "Scroll terminal output up",
        terminal_panel_scroll_top, "Scroll terminal output to top",
        terminal_panel_scroll_bottom, "Scroll terminal output to bottom",
        terminal_panel_search, "Open terminal search prompt",
        terminal_panel_tab_menu, "Toggle terminal tab menu",
        terminal_panel_close, "Close terminal panel",
        terminal_open, "Open integrated terminal",
        terminal_close, "Close integrated terminal",
        terminal_toggle, "Toggle integrated terminal",
        terminal_insert_mode, "Enter terminal insert mode",
        terminal_send, "Send editor selection or line to terminal",
        terminal_new, "Open a new terminal tab",
        terminal_list, "List and switch terminal tabs",
        jump_forward, "Jump forward on jumplist",
        jump_backward, "Jump backward on jumplist",
        save_selection, "Save current selection to jumplist",
        jump_view_right, "Jump to right split",
        jump_view_left, "Jump to left split",
        jump_view_up, "Jump to split above",
        jump_view_down, "Jump to split below",
        swap_view_right, "Swap with right split",
        swap_view_left, "Swap with left split",
        swap_view_up, "Swap with split above",
        swap_view_down, "Swap with split below",
        transpose_view, "Transpose splits",
        rotate_view, "Goto next window",
        rotate_view_reverse, "Goto previous window",
        hsplit, "Horizontal bottom split",
        hsplit_new, "Horizontal bottom split scratch buffer",
        vsplit, "Vertical right split",
        vsplit_new, "Vertical right split scratch buffer",
        wclose, "Close window",
        wonly, "Close windows except current",
        select_register, "Select register",
        insert_register, "Insert register",
        copy_between_registers, "Copy between two registers",
        align_view_middle, "Align view middle",
        align_view_top, "Align view top",
        align_view_center, "Align view center",
        align_view_bottom, "Align view bottom",
        scroll_up, "Scroll view up",
        scroll_down, "Scroll view down",
        match_brackets, "Goto matching bracket",
        surround_add, "Surround add",
        surround_replace, "Surround replace",
        surround_delete, "Surround delete",
        select_textobject_around, "Select around object",
        select_textobject_inner, "Select inside object",
        goto_next_function, "Goto next function",
        goto_prev_function, "Goto previous function",
        goto_next_class, "Goto next type definition",
        goto_prev_class, "Goto previous type definition",
        goto_next_parameter, "Goto next parameter",
        goto_prev_parameter, "Goto previous parameter",
        goto_next_comment, "Goto next comment",
        goto_prev_comment, "Goto previous comment",
        goto_next_test, "Goto next test",
        goto_prev_test, "Goto previous test",
        goto_next_xml_element, "Goto next (X)HTML element",
        goto_prev_xml_element, "Goto previous (X)HTML element",
        goto_next_entry, "Goto next pairing",
        goto_prev_entry, "Goto previous pairing",
        goto_next_paragraph, "Goto next paragraph",
        goto_prev_paragraph, "Goto previous paragraph",
        dap_launch, "Launch debug target",
        dap_restart, "Restart debugging session",
        dap_toggle_breakpoint, "Toggle breakpoint",
        dap_continue, "Continue program execution",
        dap_pause, "Pause program execution",
        dap_step_in, "Step in",
        dap_step_out, "Step out",
        dap_next, "Step to next",
        dap_variables, "List variables",
        dap_terminate, "End debug session",
        dap_edit_condition, "Edit breakpoint condition on current line",
        dap_edit_log, "Edit breakpoint log message on current line",
        dap_switch_thread, "Switch current thread",
        dap_switch_stack_frame, "Switch stack frame",
        dap_enable_exceptions, "Enable exception breakpoints",
        dap_disable_exceptions, "Disable exception breakpoints",
        shell_pipe, "Pipe selections through shell command",
        shell_pipe_to, "Pipe selections into shell command ignoring output",
        shell_insert_output, "Insert shell command output before selections",
        shell_append_output, "Append shell command output after selections",
        shell_keep_pipe, "Filter selections with shell predicate",
        suspend, "Suspend and return to shell",
        rename_symbol, "Rename symbol",
        increment, "Increment item under cursor",
        decrement, "Decrement item under cursor",
        record_macro, "Record macro",
        replay_macro, "Replay macro",
        command_palette, "Open command palette",
        goto_word, "Jump to a two-character label",
        extend_to_word, "Extend to a two-character label",
        goto_next_tabstop, "Goto next snippet placeholder",
        goto_prev_tabstop, "Goto next snippet placeholder",
        rotate_selections_first, "Make the first selection your primary one",
        rotate_selections_last, "Make the last selection your primary one",
    );
}

impl fmt::Debug for MappableCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MappableCommand::Static { name, .. } => {
                f.debug_tuple("MappableCommand").field(name).finish()
            }
            MappableCommand::Typable { name, args, .. } => f
                .debug_tuple("MappableCommand")
                .field(name)
                .field(args)
                .finish(),
            MappableCommand::Macro { name, keys, .. } => f
                .debug_tuple("MappableCommand")
                .field(name)
                .field(keys)
                .finish(),
        }
    }
}

impl fmt::Display for MappableCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

impl std::str::FromStr for MappableCommand {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(suffix) = s.strip_prefix(':') {
            let (name, args, _) = command_line::split(suffix);
            ensure!(!name.is_empty(), "Expected typable command name");
            typed::TYPABLE_COMMAND_MAP
                .get(name)
                .map(|cmd| {
                    let doc = if args.is_empty() {
                        cmd.doc.to_string()
                    } else {
                        format!(":{} {:?}", cmd.name, args)
                    };
                    MappableCommand::Typable {
                        name: cmd.name.to_owned(),
                        doc,
                        args: args.to_string(),
                    }
                })
                .ok_or_else(|| anyhow!("No TypableCommand named '{}'", s))
        } else if let Some(suffix) = s.strip_prefix('@') {
            helix_view::input::parse_macro(suffix).map(|keys| Self::Macro {
                name: s.to_string(),
                keys,
            })
        } else {
            MappableCommand::STATIC_COMMAND_LIST
                .iter()
                .find(|cmd| cmd.name() == s)
                .cloned()
                .ok_or_else(|| anyhow!("No command named '{}'", s))
        }
    }
}

impl<'de> Deserialize<'de> for MappableCommand {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(de::Error::custom)
    }
}

impl PartialEq for MappableCommand {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                MappableCommand::Typable {
                    name: first_name,
                    args: first_args,
                    ..
                },
                MappableCommand::Typable {
                    name: second_name,
                    args: second_args,
                    ..
                },
            ) => first_name == second_name && first_args == second_args,
            (
                MappableCommand::Static {
                    name: first_name, ..
                },
                MappableCommand::Static {
                    name: second_name, ..
                },
            ) => first_name == second_name,
            _ => false,
        }
    }
}
