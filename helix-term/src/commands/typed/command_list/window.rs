use helix_core::command_line::Signature;

use crate::ui::completers;
// Focus class: Document — splits and goto require a document view.

use super::{CommandCompleter, FocusRequirement, TypableCommand};
use crate::commands::typed::window as cmd;

pub(super) const COMMANDS: &[TypableCommand] = &[

    TypableCommand {
        name: "vsplit",
        aliases: &["vs"],
        doc: "Open the file in a vertical split.",
        fun: cmd::vsplit,
        completer: CommandCompleter::all(completers::filename),
        signature: Signature {
            positionals: (0, None),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Document,
    },
    TypableCommand {
        name: "vsplit-new",
        aliases: &["vnew"],
        doc: "Open a scratch buffer in a vertical split.",
        fun: cmd::vsplit_new,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Document,
    },
    TypableCommand {
        name: "hsplit",
        aliases: &["hs", "sp"],
        doc: "Open the file in a horizontal split.",
        fun: cmd::hsplit,
        completer: CommandCompleter::all(completers::filename),
        signature: Signature {
            positionals: (0, None),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Document,
    },
    TypableCommand {
        name: "hsplit-new",
        aliases: &["hnew"],
        doc: "Open a scratch buffer in a horizontal split.",
        fun: cmd::hsplit_new,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Document,
    },
    TypableCommand {
        name: "tutor",
        aliases: &[],
        doc: "Open the tutorial.",
        fun: cmd::tutor,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Document,
    },
    TypableCommand {
        name: "goto",
        aliases: &["g"],
        doc: "Goto line number.",
        fun: cmd::goto_line_number,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (1, Some(1)),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Document,
    },
];
