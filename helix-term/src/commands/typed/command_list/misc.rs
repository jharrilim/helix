use helix_core::command_line::Signature;

use crate::ui::completers;
// Focus class: Document for buffer ops; Global for registers/echo/redraw/noop.

use super::{CommandCompleter, FocusRequirement, TypableCommand};
use crate::commands::typed::misc as cmd;

pub(super) const COMMANDS: &[TypableCommand] = &[

    TypableCommand {
        name: "character-info",
        aliases: &["char"],
        doc: "Get info about the character under the primary cursor.",
        fun: cmd::get_character_info,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Document,
    },
    TypableCommand {
        name: "clear-register",
        aliases: &[],
        doc: "Clear given register. If no argument is provided, clear all registers.",
        fun: cmd::clear_register,
        completer: CommandCompleter::all(completers::register),
        signature: Signature {
            positionals: (0, Some(1)),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Global,
    },
    TypableCommand {
        name: "set-register",
        aliases: &[],
        doc: "Set contents of the given register.",
        fun: cmd::set_register,
        completer: CommandCompleter::positional(&[completers::register, completers::none]),
        signature: Signature {
            positionals: (2, Some(2)),
            raw_after: Some(1),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Global,
    },
    TypableCommand {
        name: "redraw",
        aliases: &[],
        doc: "Clear and re-render the whole UI",
        fun: cmd::redraw,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Global,
    },
    TypableCommand {
        name: "move",
        aliases: &["mv"],
        doc: "Move the current buffer and its corresponding file to a different path",
        fun: cmd::move_buffer,
        completer: CommandCompleter::positional(&[completers::filename]),
        signature: Signature {
            positionals: (1, Some(1)),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Document,
    },
    TypableCommand {
        name: "move!",
        aliases: &["mv!"],
        doc: "Move the current buffer and its corresponding file to a different path creating necessary subdirectories",
        fun: cmd::force_move_buffer,
        completer: CommandCompleter::positional(&[completers::filename]),
        signature: Signature {
            positionals: (1, Some(1)),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Document,
    },
    TypableCommand {
        name: "yank-diagnostic",
        aliases: &[],
        doc: "Yank diagnostic(s) under primary cursor to register, or clipboard by default",
        fun: cmd::yank_diagnostic,
        completer: CommandCompleter::all(completers::register),
        signature: Signature {
            positionals: (0, Some(1)),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Document,
    },
    TypableCommand {
        name: "read",
        aliases: &["r"],
        doc: "Load a file into buffer",
        fun: cmd::read,
        completer: CommandCompleter::positional(&[completers::filename]),
        signature: Signature {
            positionals: (1, Some(1)),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Document,
    },
    TypableCommand {
        name: "echo",
        aliases: &[],
        doc: "Prints the given arguments to the statusline.",
        fun: cmd::echo,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (1, None),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Global,
    },
    TypableCommand {
        name: "noop",
        aliases: &[],
        doc: "Does nothing.",
        fun: cmd::noop,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, None),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Global,
    },
];
