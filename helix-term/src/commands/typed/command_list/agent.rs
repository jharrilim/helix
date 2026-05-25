use helix_core::command_line::Signature;


use super::{CommandCompleter, FocusRequirement, TypableCommand};
use super::{AGENT_HISTORY_SIGNATURE};
use crate::commands::typed::agent as cmd;
// Focus class: Global — agent panel commands are safe from any focus.

pub const COMMANDS: &[TypableCommand] = &[
    TypableCommand {
        name: "agent-open",
        aliases: &[],
        doc: "Open the agent panel",
        fun: cmd::typed_agent_open,
        completer: CommandCompleter::none(),
        signature: Signature { positionals: (0, None), ..Signature::DEFAULT },
        focus: FocusRequirement::Global,
    },
    TypableCommand {
        name: "agent-close",
        aliases: &[],
        doc: "Close the agent panel",
        fun: cmd::typed_agent_close,
        completer: CommandCompleter::none(),
        signature: Signature { positionals: (0, None), ..Signature::DEFAULT },
        focus: FocusRequirement::Global,
    },
    TypableCommand {
        name: "agent-focus",
        aliases: &[],
        doc: "Focus the agent panel",
        fun: cmd::typed_agent_focus,
        completer: CommandCompleter::none(),
        signature: Signature { positionals: (0, None), ..Signature::DEFAULT },
        focus: FocusRequirement::Global,
    },
    TypableCommand {
        name: "agent-send",
        aliases: &[],
        doc: "Send the current agent prompt",
        fun: cmd::typed_agent_send,
        completer: CommandCompleter::none(),
        signature: Signature { positionals: (0, None), ..Signature::DEFAULT },
        focus: FocusRequirement::Global,
    },
    TypableCommand {
        name: "agent-stop",
        aliases: &[],
        doc: "Stop the agent session",
        fun: cmd::typed_agent_stop,
        completer: CommandCompleter::none(),
        signature: Signature { positionals: (0, None), ..Signature::DEFAULT },
        focus: FocusRequirement::Global,
    },
    TypableCommand {
        name: "agent-history",
        aliases: &[],
        doc: "List and load agent sessions",
        fun: cmd::typed_agent_history,
        completer: CommandCompleter::none(),
        signature: AGENT_HISTORY_SIGNATURE,
        focus: FocusRequirement::Global,
    },
    TypableCommand {
        name: "agent-new",
        aliases: &[],
        doc: "Start a new agent session",
        fun: cmd::typed_agent_new,
        completer: CommandCompleter::none(),
        signature: Signature { positionals: (0, None), ..Signature::DEFAULT },
        focus: FocusRequirement::Global,
    },
    TypableCommand {
        name: "agent-mode",
        aliases: &[],
        doc: "Set or pick the agent session mode",
        fun: cmd::typed_agent_mode,
        completer: CommandCompleter::none(),
        signature: Signature {
            positionals: (0, Some(1)),
            ..Signature::DEFAULT
        },
        focus: FocusRequirement::Global,
    },
];
