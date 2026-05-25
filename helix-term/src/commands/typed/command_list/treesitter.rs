use helix_core::command_line::Signature;


use super::{CommandCompleter, FocusRequirement, TypableCommand};
use crate::commands::typed::treesitter as cmd;
// Focus class: Document — tree-sitter introspection uses cursor position.

pub(super) const COMMANDS: &[TypableCommand] = &[

    TypableCommand {
        name: "tree-sitter-scopes",
        aliases: &[],
        doc: "Display tree sitter scopes, primarily for theming and development.",
        fun: cmd::tree_sitter_scopes,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Document,
    },
    TypableCommand {
        name: "tree-sitter-highlight-name",
        aliases: &[],
        doc: "Display name of tree-sitter highlight scope under the cursor.",
        fun: cmd::tree_sitter_highlight_name,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Document,
    },
    TypableCommand {
        name: "tree-sitter-layers",
        aliases: &[],
        doc: "Display language names of tree-sitter injection layers under the cursor.",
        fun: cmd::tree_sitter_layers,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Document,
    },
    TypableCommand {
        name: "tree-sitter-subtree",
        aliases: &["ts-subtree"],
        doc: "Display the smallest tree-sitter subtree that spans the primary selection, primarily for debugging queries.",
        fun: cmd::tree_sitter_subtree,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(0)),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Document,
    },
];
