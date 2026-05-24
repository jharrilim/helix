use helix_core::command_line::Signature;

use crate::ui::completers;

use super::{CommandCompleter, TypableCommand};
use super::{WRITE_NO_FORMAT_FLAG};
use crate::commands::typed::lsp as cmd;

pub(super) const COMMANDS: &[TypableCommand] = &[

    TypableCommand {
        name: "update",
        aliases: &["u"],
        doc: "Write changes only if the file has been modified.",
        fun: cmd::update,
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
        fun: cmd::lsp_workspace_command,
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
        fun: cmd::lsp_restart,
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
        fun: cmd::lsp_stop,
        completer: CommandCompleter::all(completers::active_language_servers),
        signature: Signature {
            positionals: (0, None),
            ..Signature::DEFAULT
        },
    },
];
