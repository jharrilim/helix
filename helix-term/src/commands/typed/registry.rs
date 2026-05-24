use helix_core::command_line::{Flag, Signature};
use crate::ui::completers;

use super::CommandCompleter;

/// This command accepts a single boolean --skip-visible flag and no positionals.
pub(crate) const BUFFER_CLOSE_OTHERS_SIGNATURE: Signature = Signature {
    positionals: (0, Some(0)),
    flags: &[Flag {
        name: "skip-visible",
        alias: Some('s'),
        doc: "don't close buffers that are visible",
        ..Flag::DEFAULT
    }],
    ..Signature::DEFAULT
};

// TODO: SHELL_SIGNATURE should specify var args for arguments, so that just completers::filename can be used,
// but Signature does not yet allow for var args.

/// This command handles all of its input as-is with no quoting or flags.
pub const SHELL_SIGNATURE: Signature = Signature {
    positionals: (1, Some(2)),
    raw_after: Some(1),
    ..Signature::DEFAULT
};

pub const SHELL_COMPLETER: CommandCompleter = CommandCompleter::positional(&[
    completers::program,
    completers::repeating_filenames,
]);

pub(crate) const WRITE_NO_FORMAT_FLAG: Flag = Flag {
    name: "no-format",
    doc: "skip auto-formatting",
    ..Flag::DEFAULT
};

pub(crate) const AGENT_HISTORY_SIGNATURE: Signature = Signature {
    positionals: (0, Some(0)),
    flags: &[Flag {
        name: "cwd",
        alias: None,
        doc: "show only sessions for the current working directory",
        ..Flag::DEFAULT
    }],
    ..Signature::DEFAULT
};
