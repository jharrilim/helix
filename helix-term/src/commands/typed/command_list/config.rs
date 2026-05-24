use helix_core::command_line::Signature;

use crate::ui::completers;

use super::{CommandCompleter, TypableCommand};
use crate::commands::typed::config as cmd;

pub(super) const COMMANDS: &[TypableCommand] = &[

    TypableCommand {
        name: "theme",
        aliases: &[],
        doc: "Change the editor theme (show current theme if no name specified).",
        fun: cmd::theme,
        completer: CommandCompleter::positional(&[completers::theme]),
        signature: Signature {
            positionals: (0, Some(1)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "set-option",
        aliases: &["set"],
        doc: "Set a config option at runtime.\nFor example to disable smart case search, use `:set search.smart-case false`.",
        fun: cmd::set_option,
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
        fun: cmd::toggle_option,
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
        fun: cmd::get_option,
        completer: CommandCompleter::positional(&[completers::setting]),
        signature: Signature {
            positionals: (1, Some(1)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "config-reload",
        aliases: &[],
        doc: "Refresh user config.",
        fun: cmd::refresh_config,
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
        fun: cmd::open_config,
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
        fun: cmd::open_workspace_config,
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
        fun: cmd::open_log,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
    },
];
