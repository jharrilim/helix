use super::{CommandCompleter, TypableCommand};

mod agent;
mod buffer;
mod clipboard;
mod config;
mod debug;
mod diff;
mod edit;
mod git;
mod lifecycle;
mod lsp;
mod misc;
mod shell;
mod terminal;
mod treesitter;
mod window;
mod workspace;

pub(super) use super::registry::{
    AGENT_HISTORY_SIGNATURE, BUFFER_CLOSE_OTHERS_SIGNATURE, SHELL_COMPLETER, SHELL_SIGNATURE,
    WRITE_NO_FORMAT_FLAG,
};

macro_rules! concat {
    ( $( $slice:expr ),* $(,)? ) => {{
        const SLICES: &[&[TypableCommand]] = &[$($slice),*];
        const fn len(slices: &[&[TypableCommand]]) -> usize {
            let mut len = 0;
            let mut i = 0;
            while i < slices.len() {
                len += slices[i].len();
                i += 1;
            }
            len
        }
        const fn flatten() -> [TypableCommand; len(SLICES)] {
            let mut result = [SLICES[0][0]; len(SLICES)];
            let mut out = 0;
            let mut si = 0;
            while si < SLICES.len() {
                let mut inner = 0;
                while inner < SLICES[si].len() {
                    result[out] = SLICES[si][inner];
                    out += 1;
                    inner += 1;
                }
                si += 1;
            }
            result
        }
        &flatten()
    }};
}

pub const TYPABLE_COMMAND_LIST: &[TypableCommand] = concat!(
    lifecycle::COMMANDS,
    buffer::COMMANDS,
    edit::COMMANDS,
    config::COMMANDS,
    workspace::COMMANDS,
    lsp::COMMANDS,
    treesitter::COMMANDS,
    window::COMMANDS,
    debug::COMMANDS,
    shell::COMMANDS,
    clipboard::COMMANDS,
    diff::COMMANDS,
    git::COMMANDS,
    misc::COMMANDS,
    agent::COMMANDS,
    terminal::COMMANDS,
);
