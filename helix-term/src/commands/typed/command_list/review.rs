use helix_core::command_line::Signature;

use super::{CommandCompleter, FocusRequirement, TypableCommand};
use crate::commands::typed::review as cmd;

pub const COMMANDS: &[TypableCommand] = &[
    TypableCommand {
        name: "review-toggle",
        aliases: &[],
        doc: "Toggle code review mode",
        fun: cmd::typed_review_toggle,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, None),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Global,
    },
    TypableCommand {
        name: "review-comment",
        aliases: &[],
        doc: "Add a review comment on the current line",
        fun: cmd::typed_review_comment,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, None),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Document,
    },
    TypableCommand {
        name: "review-submit",
        aliases: &[],
        doc: "Submit the current review to the agent",
        fun: cmd::typed_review_submit,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, None),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Global,
    },
    TypableCommand {
        name: "review-new",
        aliases: &[],
        doc: "Start a new code review session",
        fun: cmd::typed_review_new,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, None),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Global,
    },
    TypableCommand {
        name: "review-list",
        aliases: &[],
        doc: "List saved code reviews for this repository",
        fun: cmd::typed_review_list,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, None),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Global,
    },
    TypableCommand {
        name: "review-summary",
        aliases: &[],
        doc: "Set the overall summary for the current review",
        fun: cmd::typed_review_summary,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, None),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Global,
    },
    TypableCommand {
        name: "review-next",
        aliases: &[],
        doc: "Jump to the next review comment",
        fun: cmd::typed_review_next,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, None),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Global,
    },
    TypableCommand {
        name: "review-prev",
        aliases: &[],
        doc: "Jump to the previous review comment",
        fun: cmd::typed_review_prev,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, None),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Global,
    },
    TypableCommand {
        name: "review-resume",
        aliases: &[],
        doc: "Resume a saved code review by id",
        fun: cmd::typed_review_resume,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (1, Some(1)),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Global,
    },
];
