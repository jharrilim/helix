use helix_core::command_line::Signature;


use super::{CommandCompleter, TypableCommand};
use crate::commands::typed::debug as cmd;

pub(super) const COMMANDS: &[TypableCommand] = &[

    TypableCommand {
        name: "debug-start",
        aliases: &["dbg"],
        doc: "Start a debug session from a given template with given parameters.",
        fun: cmd::debug_start,
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
        fun: cmd::debug_remote,
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
        fun: cmd::debug_eval,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (1, Some(1)),
            ..Signature::DEFAULT
        },
    },
];
