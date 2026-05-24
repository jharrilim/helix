use helix_core::command_line::{Flag, Signature};

use crate::ui::completers;

use super::{CommandCompleter, TypableCommand};
use crate::commands::typed::edit as cmd;

pub(super) const COMMANDS: &[TypableCommand] = &[

    TypableCommand {
        name: "format",
        aliases: &["fmt"],
        doc: "Format the file using an external formatter or language server.",
        fun: cmd::format,
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
        fun: cmd::set_indent_style,
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
        fun: cmd::set_line_ending,
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
        fun: cmd::earlier,
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
        fun: cmd::later,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(1)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "set-language",
        aliases: &["lang"],
        doc: "Set the language of current buffer (show current language if no value specified).",
        fun: cmd::language,
        completer: CommandCompleter::positional(&[completers::language]),
        signature: Signature {
            positionals: (0, Some(1)),
            ..Signature::DEFAULT
        },
    },
    TypableCommand {
        name: "sort",
        aliases: &[],
        doc: "Sort ranges in selection.",
        fun: cmd::sort,
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
        fun: cmd::reflow,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(1)),
            ..Signature::DEFAULT
        },
    },
];
