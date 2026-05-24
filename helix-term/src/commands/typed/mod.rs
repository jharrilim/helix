use std::fmt::Write;
use std::ops::{self, Deref};

use crate::commands::prelude::*;
use crate::commands::Context;
use helix_core::command_line::{Args, Flag, Signature, Token, TokenKind};
use helix_core::fuzzy::fuzzy_match;
use helix_view::expansion;
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

use registry::*;
pub use registry::{SHELL_COMPLETER, SHELL_SIGNATURE};
use helix_core::command_line;

#[derive(Clone)]
pub struct TypableCommand {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub doc: &'static str,
    pub fun: fn(&mut compositor::Context, Args, PromptEvent) -> anyhow::Result<()>,
    pub completer: CommandCompleter,
    pub signature: Signature,
}

#[derive(Clone)]
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

pub const TYPABLE_COMMAND_LIST: &[TypableCommand] = include!("command_list.rs");

mod infra;

pub use infra::{
    command_mode, complete_command_args, execute_command, TYPABLE_COMMAND_MAP,
};
