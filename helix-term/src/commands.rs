pub(crate) mod context;
pub(crate) mod helpers;
pub(crate) mod prelude;
pub(crate) mod agent;
pub(crate) mod terminal;
pub(crate) mod dap;
pub(crate) mod fold;
pub(crate) mod lsp;
pub(crate) mod syntax;
pub(crate) mod typed;

pub(crate) mod movement;
pub(crate) mod search;
pub(crate) mod selection;
pub(crate) mod edit;
pub(crate) mod pickers;
pub(crate) mod goto;
pub mod insert;
pub(crate) mod history;
pub(crate) mod yank;
pub(crate) mod view;
pub(crate) mod textobject;
pub(crate) mod shell;
pub(crate) mod misc;

pub use context::{Context, OnKeyCallback, OnKeyCallbackKind};
pub(crate) use helpers::*;
pub use agent::*;
pub use terminal::*;
pub use dap::*;
pub use fold::*;
pub use lsp::*;
pub use syntax::*;

pub use movement::*;
pub use search::*;
pub use selection::*;
pub use edit::*;
pub use pickers::*;
pub use goto::*;
pub use insert::*;
pub use history::*;
pub use yank::*;
pub use view::*;
pub use textobject::*;
pub use shell::*;
pub use misc::*;
pub use typed::*;

mod mappable;
pub use mappable::MappableCommand;

pub use helix_view::{align_view, Align, Editor};
