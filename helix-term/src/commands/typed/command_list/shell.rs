
// Focus class: Document — shell commands use current selections.

use super::{FocusRequirement, TypableCommand};
use super::{SHELL_COMPLETER, SHELL_SIGNATURE};
use crate::commands::typed::shell as cmd;

pub(super) const COMMANDS: &[TypableCommand] = &[

    TypableCommand {
        name: "insert-output",
        aliases: &[],
        doc: "Run shell command, inserting output before each selection.",
        fun: cmd::insert_output,
        completer: SHELL_COMPLETER,
        signature: SHELL_SIGNATURE,
        focus: FocusRequirement::Document,
    },
    TypableCommand {
        name: "append-output",
        aliases: &[],
        doc: "Run shell command, appending output after each selection.",
        fun: cmd::append_output,
        completer: SHELL_COMPLETER,
        signature: SHELL_SIGNATURE,
        focus: FocusRequirement::Document,
    },
    TypableCommand {
        name: "pipe",
        aliases: &["|"],
        doc: "Pipe each selection to the shell command.",
        fun: cmd::pipe,
        completer: SHELL_COMPLETER,
        signature: SHELL_SIGNATURE,
        focus: FocusRequirement::Document,
    },
    TypableCommand {
        name: "pipe-to",
        aliases: &[],
        doc: "Pipe each selection to the shell command, ignoring output.",
        fun: cmd::pipe_to,
        completer: SHELL_COMPLETER,
        signature: SHELL_SIGNATURE,
        focus: FocusRequirement::Document,
    },
    TypableCommand {
        name: "run-shell-command",
        aliases: &["sh", "!"],
        doc: "Run a shell command",
        fun: cmd::run_shell_command,
        completer: SHELL_COMPLETER,
        signature: SHELL_SIGNATURE,
        focus: FocusRequirement::Document,
    },
];
