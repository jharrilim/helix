use helix_core::command_line::{Flag, Signature};

use crate::ui::completers;
// Focus class: Global — terminal panel commands are safe from any focus.

use super::{CommandCompleter, FocusRequirement, TypableCommand};
use crate::commands::typed::terminal as cmd;

pub(super) const COMMANDS: &[TypableCommand] = &[

    TypableCommand {
        name: "terminal-open",
        aliases: &["term-open"],
        doc: "Open the integrated terminal panel",
        fun: cmd::typed_terminal_open,
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
        focus: FocusRequirement::Global,
    },
    TypableCommand {
        name: "terminal-close",
        aliases: &["term-close"],
        doc: "Close the active terminal tab",
        fun: cmd::typed_terminal_close,
        completer: CommandCompleter::none(),
        signature: Signature { positionals: (0, None), ..Signature::DEFAULT },
        focus: FocusRequirement::Global,
    },
    TypableCommand {
        name: "terminal-new",
        aliases: &["term-new"],
        doc: "Open a new terminal tab",
        fun: cmd::typed_terminal_new,
        completer: CommandCompleter::none(),
        signature: Signature { positionals: (0, None), ..Signature::DEFAULT },
        focus: FocusRequirement::Global,
    },
    TypableCommand {
        name: "terminal-list",
        aliases: &["term-list"],
        doc: "List and switch terminal tabs",
        fun: cmd::typed_terminal_list,
        completer: CommandCompleter::none(),
        signature: Signature { positionals: (0, None), ..Signature::DEFAULT },
        focus: FocusRequirement::Global,
    },
    TypableCommand {
        name: "terminal-focus",
        aliases: &["term-focus"],
        doc: "Focus a terminal tab by session id",
        fun: cmd::typed_terminal_focus,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (1, Some(1)),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Global,
    },
    TypableCommand {
        name: "terminal-toggle",
        aliases: &["term-toggle"],
        doc: "Toggle the integrated terminal panel",
        fun: cmd::typed_terminal_toggle,
        completer: CommandCompleter::none(),
        signature: Signature { positionals: (0, None), ..Signature::DEFAULT },
        focus: FocusRequirement::Global,
    },
    TypableCommand {
        name: "terminal-send",
        aliases: &["term-send"],
        doc: "Send the editor selection or current line to the active terminal",
        fun: cmd::typed_terminal_send,
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
        focus: FocusRequirement::Global,
    },
];
