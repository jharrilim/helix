pub use futures_util::FutureExt;
pub use helix_event::status;
pub use helix_stdx::{
    path::{self, find_paths},
    rope::{self, RopeSliceExt},
};
pub use helix_vcs::{FileChange, Hunk};
pub use tui::{
    text::{Span, Spans},
    widgets::Cell,
};

pub use helix_core::{
    char_idx_at_visual_offset,
    chars::char_is_word,
    command_line::Args,
    comment,
    doc_formatter::TextFormat,
    encoding, find_workspace,
    graphemes::{self, next_grapheme_boundary},
    history::UndoKind,
    increment,
    indent::{self, IndentStyle},
    line_ending::{get_line_ending_of_str, line_end_char_index, LineEnding},
    match_brackets,
    movement::{self, move_vertically_visual, Direction, Movement},
    object, pos_at_coords,
    regex::{self, Regex},
    search::{self},
    selection, surround,
    syntax::config::{BlockCommentToken, LanguageServerFeature},
    text_annotations::{Overlay, TextAnnotations},
    textobject,
    unicode::width::UnicodeWidthChar,
    visual_offset_from_block, Deletion, Position, Range, Rope, RopeReader, RopeSlice,
    Selection, SmallVec, Syntax, Tendril, Transaction,
};
pub use helix_view::{
    align_view, document::{FormatterError, Mode, SCRATCH_BUFFER_NAME},
    editor::{Action, Motion},
    expansion,
    info::Info,
    input::KeyEvent,
    keyboard::KeyCode,
    theme::Style,
    tree,
    view::View,
    Align, Document, DocumentId, Editor, ViewId,
};

pub use anyhow::{anyhow, bail, ensure, Context as _};
pub use arc_swap::access::DynAccess;

pub use crate::{
    compositor::{self, Component, Compositor},
    job::Callback,
    ui::{self, overlay::overlaid, Picker, PickerColumn, Popup, Prompt, PromptEvent},
};

pub(crate) use crate::filter_picker_entry;

pub use crate::job::{self};
pub use std::{
    borrow::Cow,
    char::{ToLowercase, ToUppercase},
    cmp::Ordering,
    collections::HashSet,
    error::Error,
    future::Future,
    io::Read,
    num::NonZeroUsize,
    path::{Path, PathBuf},
};

pub use once_cell::sync::Lazy;
pub use url::Url;

pub use grep_regex::RegexMatcherBuilder;
pub use grep_searcher::{sinks, BinaryDetection, SearcherBuilder};
pub use ignore::{DirEntry, WalkBuilder, WalkState};
