use helix_core::command_line::Signature;


use super::{CommandCompleter, FocusRequirement, TypableCommand};
use crate::commands::typed::diff as cmd;
// Focus class: Document — diff reset uses cursor position.

pub(super) const COMMANDS: &[TypableCommand] = &[

    TypableCommand {
        name: "reset-diff-change",
        aliases: &["diffget", "diffg"],
        doc: "Reset the diff change at the cursor position.",
        fun: cmd::reset_diff_change,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Document,
    },
];
