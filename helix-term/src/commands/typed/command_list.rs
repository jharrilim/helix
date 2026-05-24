&[
    TypableCommand {
        name: "exit",
        aliases: &["x", "xit"],
        doc: "Write changes to disk if the buffer is modified and then quit. Accepts an optional path (:exit some/path.txt).",
        fun: lifecycle::exit,
        completer: CommandCompleter::positional(&[completers::filename]),
        signature: Signature {
            positionals: (0, Some(1)),
            flags: &[WRITE_NO_FORMAT_FLAG],
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "exit!",
        aliases: &["x!", "xit!"],
        doc: "Force write changes to disk, creating necessary subdirectories, if the buffer is modified and then quit. Accepts an optional path (:exit! some/path.txt).",
        fun: lifecycle::force_exit,
        completer: CommandCompleter::positional(&[completers::filename]),
        signature: Signature {
            positionals: (0, Some(1)),
            flags: &[WRITE_NO_FORMAT_FLAG],
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "quit",
        aliases: &["q"],
        doc: "Close the current view.",
        fun: lifecycle::quit,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "quit!",
        aliases: &["q!"],
        doc: "Force close the current view, ignoring unsaved changes.",
        fun: lifecycle::force_quit,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "open",
        aliases: &["o", "edit", "e"],
        doc: "Open a file from disk into the current view.",
        fun: buffer::open,
        completer: CommandCompleter::all(completers::filename),
        signature: Signature {
            positionals: (1, None),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "buffer-close",
        aliases: &["bc", "bclose"],
        doc: "Close the current buffer.",
        fun: buffer::buffer_close,
        completer: CommandCompleter::all(completers::buffer),
        signature: Signature {
            positionals: (0, None),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "buffer-close!",
        aliases: &["bc!", "bclose!"],
        doc: "Close the current buffer forcefully, ignoring unsaved changes.",
        fun: buffer::force_buffer_close,
        completer: CommandCompleter::all(completers::buffer),
        signature: Signature {
            positionals: (0, None),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "buffer-close-others",
        aliases: &["bco", "bcloseother"],
        doc: "Close all buffers but the currently focused one.",
        fun: buffer::buffer_close_others,
        completer: CommandCompleter::none(),
        signature: BUFFER_CLOSE_OTHERS_SIGNATURE,
    },
    TypableCommand {
        name: "buffer-close-others!",
        aliases: &["bco!", "bcloseother!"],
        doc: "Force close all buffers but the currently focused one.",
        fun: buffer::force_buffer_close_others,
        completer: CommandCompleter::none(),
        signature: BUFFER_CLOSE_OTHERS_SIGNATURE,
    },
    TypableCommand {
        name: "buffer-close-all",
        aliases: &["bca", "bcloseall"],
        doc: "Close all buffers without quitting.",
        fun: buffer::buffer_close_all,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "buffer-close-all!",
        aliases: &["bca!", "bcloseall!"],
        doc: "Force close all buffers ignoring unsaved changes without quitting.",
        fun: buffer::force_buffer_close_all,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "buffer-next",
        aliases: &["bn", "bnext"],
        doc: "Goto next buffer.",
        fun: buffer::buffer_next,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "buffer-previous",
        aliases: &["bp", "bprev"],
        doc: "Goto previous buffer.",
        fun: buffer::buffer_previous,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "write",
        aliases: &["w"],
        doc: "Write changes to disk. Accepts an optional path (:write some/path.txt)",
        fun: lifecycle::write,
        completer: CommandCompleter::positional(&[completers::filename]),
        signature: Signature {
            positionals: (0, Some(1)),
            flags: &[WRITE_NO_FORMAT_FLAG],
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "write!",
        aliases: &["w!"],
        doc: "Force write changes to disk creating necessary subdirectories. Accepts an optional path (:write! some/path.txt)",
        fun: lifecycle::force_write,
        completer: CommandCompleter::positional(&[completers::filename]),
        signature: Signature {
            positionals: (0, Some(1)),
            flags: &[WRITE_NO_FORMAT_FLAG],
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "write-buffer-close",
        aliases: &["wbc"],
        doc: "Write changes to disk and closes the buffer. Accepts an optional path (:write-buffer-close some/path.txt)",
        fun: lifecycle::write_buffer_close,
        completer: CommandCompleter::positional(&[completers::filename]),
        signature: Signature {
            positionals: (0, Some(1)),
            flags: &[WRITE_NO_FORMAT_FLAG],
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "write-buffer-close!",
        aliases: &["wbc!"],
        doc: "Force write changes to disk creating necessary subdirectories and closes the buffer. Accepts an optional path (:write-buffer-close! some/path.txt)",
        fun: lifecycle::force_write_buffer_close,
        completer: CommandCompleter::positional(&[completers::filename]),
        signature: Signature {
            positionals: (0, Some(1)),
            flags: &[WRITE_NO_FORMAT_FLAG],
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "new",
        aliases: &["n"],
        doc: "Create a new scratch buffer.",
        fun: buffer::new_file,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "format",
        aliases: &["fmt"],
        doc: "Format the file using an external formatter or language server.",
        fun: edit::format,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "indent-style",
        aliases: &[],
        doc: "Set the indentation style for editing. ('t' for tabs or 1-16 for number of spaces.)",
        fun: edit::set_indent_style,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(1)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "line-ending",
        aliases: &[],
        #[cfg(not(feature = "unicode-lines"))]
        doc: "Set the document's default line ending. Options: crlf, lf.",
        #[cfg(feature = "unicode-lines")]
        doc: "Set the document's default line ending. Options: crlf, lf, cr, ff, nel.",
        fun: edit::set_line_ending,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(1)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "earlier",
        aliases: &["ear"],
        doc: "Jump back to an earlier point in edit history. Accepts a number of steps or a time span.",
        fun: edit::earlier,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(1)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "later",
        aliases: &["lat"],
        doc: "Jump to a later point in edit history. Accepts a number of steps or a time span.",
        fun: edit::later,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(1)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "write-quit",
        aliases: &["wq"],
        doc: "Write changes to disk and close the current view. Accepts an optional path (:wq some/path.txt)",
        fun: lifecycle::write_quit,
        completer: CommandCompleter::positional(&[completers::filename]),
        signature: Signature {
            positionals: (0, Some(1)),
            flags: &[WRITE_NO_FORMAT_FLAG],
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "write-quit!",
        aliases: &["wq!"],
        doc: "Write changes to disk and close the current view forcefully. Accepts an optional path (:wq! some/path.txt)",
        fun: lifecycle::force_write_quit,
        completer: CommandCompleter::positional(&[completers::filename]),
        signature: Signature {
            positionals: (0, Some(1)),
            flags: &[WRITE_NO_FORMAT_FLAG],
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "write-all",
        aliases: &["wa"],
        doc: "Write changes from all buffers to disk.",
        fun: lifecycle::write_all,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            flags: &[WRITE_NO_FORMAT_FLAG],
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "write-all!",
        aliases: &["wa!"],
        doc: "Forcefully write changes from all buffers to disk creating necessary subdirectories.",
        fun: lifecycle::force_write_all,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            flags: &[WRITE_NO_FORMAT_FLAG],
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "write-quit-all",
        aliases: &["wqa", "xa"],
        doc: "Write changes from all buffers to disk and close all views.",
        fun: lifecycle::write_all_quit,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            flags: &[WRITE_NO_FORMAT_FLAG],
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "write-quit-all!",
        aliases: &["wqa!", "xa!"],
        doc: "Forcefully write changes from all buffers to disk, creating necessary subdirectories, and close all views (ignoring unsaved changes).",
        fun: lifecycle::force_write_all_quit,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            flags: &[WRITE_NO_FORMAT_FLAG],
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "quit-all",
        aliases: &["qa"],
        doc: "Close all views.",
        fun: lifecycle::quit_all,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "quit-all!",
        aliases: &["qa!"],
        doc: "Force close all views ignoring unsaved changes.",
        fun: lifecycle::force_quit_all,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "cquit",
        aliases: &["cq"],
        doc: "Quit with exit code (default 1). Accepts an optional integer exit code (:cq 2).",
        fun: lifecycle::cquit,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(1)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "cquit!",
        aliases: &["cq!"],
        doc: "Force quit with exit code (default 1) ignoring unsaved changes. Accepts an optional integer exit code (:cq! 2).",
        fun: lifecycle::force_cquit,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(1)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "theme",
        aliases: &[],
        doc: "Change the editor theme (show current theme if no name specified).",
        fun: config::theme,
        completer: CommandCompleter::positional(&[completers::theme]),
        signature: Signature {
            positionals: (0, Some(1)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "yank-join",
        aliases: &[],
        doc: "Yank joined selections. A separator can be provided as first argument. Default value is newline.",
        fun: clipboard::yank_joined,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(1)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "clipboard-yank",
        aliases: &[],
        doc: "Yank main selection into system clipboard.",
        fun: clipboard::yank_main_selection_to_clipboard,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "clipboard-yank-join",
        aliases: &[],
        doc: "Yank joined selections into system clipboard. A separator can be provided as first argument. Default value is newline.", // FIXME: current UI can't display long doc.
        fun: clipboard::yank_joined_to_clipboard,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(1)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "primary-clipboard-yank",
        aliases: &[],
        doc: "Yank main selection into system primary clipboard.",
        fun: clipboard::yank_main_selection_to_primary_clipboard,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "primary-clipboard-yank-join",
        aliases: &[],
        doc: "Yank joined selections into system primary clipboard. A separator can be provided as first argument. Default value is newline.", // FIXME: current UI can't display long doc.
        fun: clipboard::yank_joined_to_primary_clipboard,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(1)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "clipboard-paste-after",
        aliases: &[],
        doc: "Paste system clipboard after selections.",
        fun: clipboard::paste_clipboard_after,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "clipboard-paste-before",
        aliases: &[],
        doc: "Paste system clipboard before selections.",
        fun: clipboard::paste_clipboard_before,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "clipboard-paste-replace",
        aliases: &[],
        doc: "Replace selections with content of system clipboard.",
        fun: clipboard::replace_selections_with_clipboard,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "primary-clipboard-paste-after",
        aliases: &[],
        doc: "Paste primary clipboard after selections.",
        fun: clipboard::paste_primary_clipboard_after,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "primary-clipboard-paste-before",
        aliases: &[],
        doc: "Paste primary clipboard before selections.",
        fun: clipboard::paste_primary_clipboard_before,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "primary-clipboard-paste-replace",
        aliases: &[],
        doc: "Replace selections with content of system primary clipboard.",
        fun: clipboard::replace_selections_with_primary_clipboard,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "show-clipboard-provider",
        aliases: &[],
        doc: "Show clipboard provider name in status bar.",
        fun: clipboard::show_clipboard_provider,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "change-current-directory",
        aliases: &["cd"],
        doc: "Change the current working directory.",
        fun: workspace::change_current_directory,
        completer: CommandCompleter::positional(&[completers::directory]),
        signature: Signature {
            positionals: (1, Some(1)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "show-directory-stack",
        aliases: &[],
        doc: "Show the directory stack as a <space> delimited string.",
        fun: workspace::show_directory_stack,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "push-directory",
        aliases: &["pushd"],
        doc: "Save and then change the current directory.",
        fun: workspace::push_directory,
        completer: CommandCompleter::positional(&[completers::directory]),
        signature: Signature {
            positionals: (1, Some(1)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "pop-directory",
        aliases: &["popd"],
        doc: "Remove the top entry from the directory stack, and cd to the new top directory..",
        fun: workspace::pop_directory,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "show-directory",
        aliases: &["pwd"],
        doc: "Show the current working directory.",
        fun: workspace::show_current_directory,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "encoding",
        aliases: &[],
        doc: "Set encoding. Based on `https://encoding.spec.whatwg.org`.",
        fun: buffer::set_encoding,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(1)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "character-info",
        aliases: &["char"],
        doc: "Get info about the character under the primary cursor.",
        fun: misc::get_character_info,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "reload",
        aliases: &["rl"],
        doc: "Discard changes and reload from the source file.",
        fun: buffer::reload,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "reload-all",
        aliases: &["rla"],
        doc: "Discard changes and reload all documents from the source files.",
        fun: buffer::reload_all,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "update",
        aliases: &["u"],
        doc: "Write changes only if the file has been modified.",
        fun: lsp::update,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            flags: &[WRITE_NO_FORMAT_FLAG],
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "lsp-workspace-command",
        aliases: &[],
        doc: "Open workspace command picker",
        fun: lsp::lsp_workspace_command,
        completer: CommandCompleter::positional(&[completers::lsp_workspace_command]),
        signature: Signature {
            positionals: (0, None),
            raw_after: Some(1),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "lsp-restart",
        aliases: &[],
        doc: "Restarts the given language servers, or all language servers that are used by the current file if no arguments are supplied",
        fun: lsp::lsp_restart,
        completer: CommandCompleter::all(completers::configured_language_servers),
        signature: Signature {
            positionals: (0, None),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "lsp-stop",
        aliases: &[],
        doc: "Stops the given language servers, or all language servers that are used by the current file if no arguments are supplied",
        fun: lsp::lsp_stop,
        completer: CommandCompleter::all(completers::active_language_servers),
        signature: Signature {
            positionals: (0, None),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "tree-sitter-scopes",
        aliases: &[],
        doc: "Display tree sitter scopes, primarily for theming and development.",
        fun: treesitter::tree_sitter_scopes,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "tree-sitter-highlight-name",
        aliases: &[],
        doc: "Display name of tree-sitter highlight scope under the cursor.",
        fun: treesitter::tree_sitter_highlight_name,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "tree-sitter-layers",
        aliases: &[],
        doc: "Display language names of tree-sitter injection layers under the cursor.",
        fun: treesitter::tree_sitter_layers,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "debug-start",
        aliases: &["dbg"],
        doc: "Start a debug session from a given template with given parameters.",
        fun: debug::debug_start,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, None),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "debug-remote",
        aliases: &["dbg-tcp"],
        doc: "Connect to a debug adapter by TCP address and start a debugging session from a given template with given parameters.",
        fun: debug::debug_remote,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, None),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "debug-eval",
        aliases: &[],
        doc: "Evaluate expression in current debug context.",
        fun: debug::debug_eval,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (1, Some(1)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "vsplit",
        aliases: &["vs"],
        doc: "Open the file in a vertical split.",
        fun: window::vsplit,
        completer: CommandCompleter::all(completers::filename),
        signature: Signature {
            positionals: (0, None),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "vsplit-new",
        aliases: &["vnew"],
        doc: "Open a scratch buffer in a vertical split.",
        fun: window::vsplit_new,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "hsplit",
        aliases: &["hs", "sp"],
        doc: "Open the file in a horizontal split.",
        fun: window::hsplit,
        completer: CommandCompleter::all(completers::filename),
        signature: Signature {
            positionals: (0, None),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "hsplit-new",
        aliases: &["hnew"],
        doc: "Open a scratch buffer in a horizontal split.",
        fun: window::hsplit_new,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "tutor",
        aliases: &[],
        doc: "Open the tutorial.",
        fun: window::tutor,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "goto",
        aliases: &["g"],
        doc: "Goto line number.",
        fun: window::goto_line_number,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (1, Some(1)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "set-language",
        aliases: &["lang"],
        doc: "Set the language of current buffer (show current language if no value specified).",
        fun: edit::language,
        completer: CommandCompleter::positional(&[completers::language]),
        signature: Signature {
            positionals: (0, Some(1)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "set-option",
        aliases: &["set"],
        doc: "Set a config option at runtime.\nFor example to disable smart case search, use `:set search.smart-case false`.",
        fun: config::set_option,
        // TODO: Add support for completion of the options value(s), when appropriate.
        completer: CommandCompleter::positional(&[completers::setting]),
        signature: Signature {
            positionals: (2, Some(2)),
            raw_after: Some(1),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "toggle-option",
        aliases: &["toggle"],
        doc: "Toggle a config option at runtime.\nFor example to toggle smart case search, use `:toggle search.smart-case`.",
        fun: config::toggle_option,
        completer: CommandCompleter::positional(&[completers::setting]),
        signature: Signature {
            positionals: (1, None),
            raw_after: Some(1),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "get-option",
        aliases: &["get"],
        doc: "Get the current value of a config option.",
        fun: config::get_option,
        completer: CommandCompleter::positional(&[completers::setting]),
        signature: Signature {
            positionals: (1, Some(1)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "sort",
        aliases: &[],
        doc: "Sort ranges in selection.",
        fun: edit::sort,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            flags: &[
                Flag {
                    name: "insensitive",
                    alias: Some('i'),
                    doc: "sort the ranges case-insensitively",
                    ..Flag::DEFAULT
                },
                Flag {
                    name: "reverse",
                    alias: Some('r'),
                    doc: "sort ranges in reverse order",
                    ..Flag::DEFAULT
                },
            ],
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "reflow",
        aliases: &[],
        doc: "Hard-wrap the current selection of lines to a given width.",
        fun: edit::reflow,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(1)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "tree-sitter-subtree",
        aliases: &["ts-subtree"],
        doc: "Display the smallest tree-sitter subtree that spans the primary selection, primarily for debugging queries.",
        fun: treesitter::tree_sitter_subtree,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "config-reload",
        aliases: &[],
        doc: "Refresh user config.",
        fun: config::refresh_config,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "config-open",
        aliases: &[],
        doc: "Open the user config.toml file.",
        fun: config::open_config,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "config-open-workspace",
        aliases: &[],
        doc: "Open the workspace config.toml file.",
        fun: config::open_workspace_config,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "log-open",
        aliases: &[],
        doc: "Open the helix log file.",
        fun: config::open_log,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "insert-output",
        aliases: &[],
        doc: "Run shell command, inserting output before each selection.",
        fun: shell::insert_output,
        completer: SHELL_COMPLETER,
        signature: SHELL_SIGNATURE,
    },
    TypableCommand {
        name: "append-output",
        aliases: &[],
        doc: "Run shell command, appending output after each selection.",
        fun: shell::append_output,
        completer: SHELL_COMPLETER,
        signature: SHELL_SIGNATURE,
    },
    TypableCommand {
        name: "pipe",
        aliases: &["|"],
        doc: "Pipe each selection to the shell command.",
        fun: shell::pipe,
        completer: SHELL_COMPLETER,
        signature: SHELL_SIGNATURE,
    },
    TypableCommand {
        name: "pipe-to",
        aliases: &[],
        doc: "Pipe each selection to the shell command, ignoring output.",
        fun: shell::pipe_to,
        completer: SHELL_COMPLETER,
        signature: SHELL_SIGNATURE,
    },
    TypableCommand {
        name: "run-shell-command",
        aliases: &["sh", "!"],
        doc: "Run a shell command",
        fun: shell::run_shell_command,
        completer: SHELL_COMPLETER,
        signature: SHELL_SIGNATURE,
    },
    TypableCommand {
        name: "reset-diff-change",
        aliases: &["diffget", "diffg"],
        doc: "Reset the diff change at the cursor position.",
        fun: diff::reset_diff_change,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "clear-register",
        aliases: &[],
        doc: "Clear given register. If no argument is provided, clear all registers.",
        fun: misc::clear_register,
        completer: CommandCompleter::all(completers::register),
        signature: Signature {
            positionals: (0, Some(1)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "set-register",
        aliases: &[],
        doc: "Set contents of the given register.",
        fun: misc::set_register,
        completer: CommandCompleter::positional(&[completers::register, completers::none]),
        signature: Signature {
            positionals: (2, Some(2)),
            raw_after: Some(1),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "redraw",
        aliases: &[],
        doc: "Clear and re-render the whole UI",
        fun: misc::redraw,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "move",
        aliases: &["mv"],
        doc: "Move the current buffer and its corresponding file to a different path",
        fun: misc::move_buffer,
        completer: CommandCompleter::positional(&[completers::filename]),
        signature: Signature {
            positionals: (1, Some(1)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "move!",
        aliases: &["mv!"],
        doc: "Move the current buffer and its corresponding file to a different path creating necessary subdirectories",
        fun: misc::force_move_buffer,
        completer: CommandCompleter::positional(&[completers::filename]),
        signature: Signature {
            positionals: (1, Some(1)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "yank-diagnostic",
        aliases: &[],
        doc: "Yank diagnostic(s) under primary cursor to register, or clipboard by default",
        fun: misc::yank_diagnostic,
        completer: CommandCompleter::all(completers::register),
        signature: Signature {
            positionals: (0, Some(1)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "read",
        aliases: &["r"],
        doc: "Load a file into buffer",
        fun: misc::read,
        completer: CommandCompleter::positional(&[completers::filename]),
        signature: Signature {
            positionals: (1, Some(1)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "echo",
        aliases: &[],
        doc: "Prints the given arguments to the statusline.",
        fun: misc::echo,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (1, None),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "noop",
        aliases: &[],
        doc: "Does nothing.",
        fun: misc::noop,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, None),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "workspace-trust",
        aliases: &[],
        doc: "Add current workspace to the list of trusted workspaces.",
        fun: workspace::trust_workspace,
        completer: CommandCompleter::none(),
        signature: Signature { positionals: (0, None), ..Signature::DEFAULT },
    },
    TypableCommand {
        name: "workspace-untrust",
        aliases: &[],
        doc: "Remove current workspace from the list of trusted workspaces.",
        fun: workspace::untrust_workspace,
        completer: CommandCompleter::none(),
        signature: Signature { positionals: (0, None), ..Signature::DEFAULT },
    },
    TypableCommand {
        name: "agent-open",
        aliases: &[],
        doc: "Open the agent panel",
        fun: agent::typed_agent_open,
        completer: CommandCompleter::none(),
        signature: Signature { positionals: (0, None), ..Signature::DEFAULT },
    },
    TypableCommand {
        name: "agent-close",
        aliases: &[],
        doc: "Close the agent panel",
        fun: agent::typed_agent_close,
        completer: CommandCompleter::none(),
        signature: Signature { positionals: (0, None), ..Signature::DEFAULT },
    },
    TypableCommand {
        name: "agent-focus",
        aliases: &[],
        doc: "Focus the agent panel",
        fun: agent::typed_agent_focus,
        completer: CommandCompleter::none(),
        signature: Signature { positionals: (0, None), ..Signature::DEFAULT },
    },
    TypableCommand {
        name: "agent-send",
        aliases: &[],
        doc: "Send the current agent prompt",
        fun: agent::typed_agent_send,
        completer: CommandCompleter::none(),
        signature: Signature { positionals: (0, None), ..Signature::DEFAULT },
    },
    TypableCommand {
        name: "agent-stop",
        aliases: &[],
        doc: "Stop the agent session",
        fun: agent::typed_agent_stop,
        completer: CommandCompleter::none(),
        signature: Signature { positionals: (0, None), ..Signature::DEFAULT },
    },
    TypableCommand {
        name: "agent-history",
        aliases: &[],
        doc: "List and load agent sessions",
        fun: agent::typed_agent_history,
        completer: CommandCompleter::none(),
        signature: AGENT_HISTORY_SIGNATURE,
    },
    TypableCommand {
        name: "agent-new",
        aliases: &[],
        doc: "Start a new agent session",
        fun: agent::typed_agent_new,
        completer: CommandCompleter::none(),
        signature: Signature { positionals: (0, None), ..Signature::DEFAULT },
    },
    TypableCommand {
        name: "agent-mode",
        aliases: &[],
        doc: "Set or pick the agent session mode",
        fun: agent::typed_agent_mode,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(1)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "terminal-open",
        aliases: &["term-open"],
        doc: "Open the integrated terminal panel",
        fun: terminal::typed_terminal_open,
        completer: CommandCompleter::positional(&[completers::directory]),
        signature: Signature {
            positionals: (0, None),
            flags: &[Flag {
                name: "cwd",
                alias: None,
                doc: "working directory for the new terminal session",
                completions: Some(&[]),
            }],
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "terminal-close",
        aliases: &["term-close"],
        doc: "Close the active terminal tab",
        fun: terminal::typed_terminal_close,
        completer: CommandCompleter::none(),
        signature: Signature { positionals: (0, None), ..Signature::DEFAULT },
    },
    TypableCommand {
        name: "terminal-new",
        aliases: &["term-new"],
        doc: "Open a new terminal tab",
        fun: terminal::typed_terminal_new,
        completer: CommandCompleter::none(),
        signature: Signature { positionals: (0, None), ..Signature::DEFAULT },
    },
    TypableCommand {
        name: "terminal-list",
        aliases: &["term-list"],
        doc: "List and switch terminal tabs",
        fun: terminal::typed_terminal_list,
        completer: CommandCompleter::none(),
        signature: Signature { positionals: (0, None), ..Signature::DEFAULT },
    },
    TypableCommand {
        name: "terminal-focus",
        aliases: &["term-focus"],
        doc: "Focus a terminal tab by session id",
        fun: terminal::typed_terminal_focus,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (1, Some(1)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "terminal-toggle",
        aliases: &["term-toggle"],
        doc: "Toggle the integrated terminal panel",
        fun: terminal::typed_terminal_toggle,
        completer: CommandCompleter::none(),
        signature: Signature { positionals: (0, None), ..Signature::DEFAULT },
    },
    TypableCommand {
        name: "terminal-send",
        aliases: &["term-send"],
        doc: "Send the editor selection or current line to the active terminal",
        fun: terminal::typed_terminal_send,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, None),
            flags: &[Flag {
                name: "no-newline",
                alias: None,
                doc: "do not append a newline after the sent text",
                ..Flag::DEFAULT
            }],
            ..Signature::DEFAULT
        },
    },
]
