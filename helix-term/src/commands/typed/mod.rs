
use crate::commands::prelude::*;
use helix_core::command_line::{Args, Signature};
use crate::ui::completers::{self, Completer};

mod registry;

mod lifecycle;
mod buffer;
mod edit;
mod config;
mod workspace;
mod lsp;
mod treesitter;
mod window;
mod debug;
mod shell;
mod clipboard;
mod diff;
mod misc;
mod agent;
mod terminal;

pub use lifecycle::{write_all_impl, WriteAllOptions, WriteOptions};
pub(crate) use lifecycle::buffers_remaining_impl;

pub use registry::{SHELL_COMPLETER, SHELL_SIGNATURE};

#[derive(Clone, Copy)]
pub struct TypableCommand {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub doc: &'static str,
    pub fun: fn(&mut compositor::Context, Args, PromptEvent) -> anyhow::Result<()>,
    pub completer: CommandCompleter,
    pub signature: Signature,
}

#[derive(Clone, Copy)]
pub struct CommandCompleter {
    positional_args: &'static [Completer],
    var_args: Completer,
}

impl CommandCompleter {
    const fn none() -> Self {
        Self {
            positional_args: &[],
            var_args: completers::none,
        }
    }

    const fn positional(completers: &'static [Completer]) -> Self {
        Self {
            positional_args: completers,
            var_args: completers::none,
        }
    }

    const fn all(completer: Completer) -> Self {
        Self {
            positional_args: &[],
            var_args: completer,
        }
    }

    fn for_argument_number(&self, n: usize) -> &Completer {
        match self.positional_args.get(n) {
            Some(completer) => completer,
            _ => &self.var_args,
        }
    }
}

mod command_list;
mod infra;

pub use command_list::TYPABLE_COMMAND_LIST;

pub use infra::{
    command_mode, complete_command_args, execute_command, TYPABLE_COMMAND_MAP,
};
