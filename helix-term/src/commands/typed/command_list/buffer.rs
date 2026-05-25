use helix_core::command_line::Signature;

use crate::ui::completers;
// Focus class: Document — all buffer operations require a document view.

use super::{CommandCompleter, FocusRequirement, TypableCommand};
use super::{BUFFER_CLOSE_OTHERS_SIGNATURE};
use crate::commands::typed::buffer as cmd;

pub(super) const COMMANDS: &[TypableCommand] = &[

    TypableCommand {
        name: "open",
        aliases: &["o", "edit", "e"],
        doc: "Open a file from disk into the current view.",
        fun: cmd::open,
        completer: CommandCompleter::all(completers::filename),
        signature: Signature {
            positionals: (1, None),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Document,
    },
    TypableCommand {
        name: "buffer-close",
        aliases: &["bc", "bclose"],
        doc: "Close the current buffer.",
        fun: cmd::buffer_close,
        completer: CommandCompleter::all(completers::buffer),
        signature: Signature {
            positionals: (0, None),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Document,
    },
    TypableCommand {
        name: "buffer-close!",
        aliases: &["bc!", "bclose!"],
        doc: "Close the current buffer forcefully, ignoring unsaved changes.",
        fun: cmd::force_buffer_close,
        completer: CommandCompleter::all(completers::buffer),
        signature: Signature {
            positionals: (0, None),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Document,
    },
    TypableCommand {
        name: "buffer-close-others",
        aliases: &["bco", "bcloseother"],
        doc: "Close all buffers but the currently focused one.",
        fun: cmd::buffer_close_others,
        completer: CommandCompleter::none(),
        signature: BUFFER_CLOSE_OTHERS_SIGNATURE,
        focus: FocusRequirement::Document,
    },
    TypableCommand {
        name: "buffer-close-others!",
        aliases: &["bco!", "bcloseother!"],
        doc: "Force close all buffers but the currently focused one.",
        fun: cmd::force_buffer_close_others,
        completer: CommandCompleter::none(),
        signature: BUFFER_CLOSE_OTHERS_SIGNATURE,
        focus: FocusRequirement::Document,
    },
    TypableCommand {
        name: "buffer-close-all",
        aliases: &["bca", "bcloseall"],
        doc: "Close all buffers without quitting.",
        fun: cmd::buffer_close_all,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Document,
    },
    TypableCommand {
        name: "buffer-close-all!",
        aliases: &["bca!", "bcloseall!"],
        doc: "Force close all buffers ignoring unsaved changes without quitting.",
        fun: cmd::force_buffer_close_all,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Document,
    },
    TypableCommand {
        name: "buffer-next",
        aliases: &["bn", "bnext"],
        doc: "Goto next buffer.",
        fun: cmd::buffer_next,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Document,
    },
    TypableCommand {
        name: "buffer-previous",
        aliases: &["bp", "bprev"],
        doc: "Goto previous buffer.",
        fun: cmd::buffer_previous,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Document,
    },
    TypableCommand {
        name: "new",
        aliases: &["n"],
        doc: "Create a new scratch buffer.",
        fun: cmd::new_file,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Document,
    },
    TypableCommand {
        name: "encoding",
        aliases: &[],
        doc: "Set encoding. Based on `https://encoding.spec.whatwg.org`.",
        fun: cmd::set_encoding,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(1)),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Document,
    },
    TypableCommand {
        name: "reload",
        aliases: &["rl"],
        doc: "Discard changes and reload from the source file.",
        fun: cmd::reload,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Document,
    },
    TypableCommand {
        name: "reload-all",
        aliases: &["rla"],
        doc: "Discard changes and reload all documents from the source files.",
        fun: cmd::reload_all,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Document,
    },
];
