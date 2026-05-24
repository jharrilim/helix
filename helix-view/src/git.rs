//! Git panel UI state stored on the editor.

use helix_vcs::{GitStatusEntry, StagingSection};

use crate::{graphics::Rect, ViewId};

/// Identifies a selected row in the git panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GitSelection {
    pub section: StagingSection,
    pub index: usize,
}

/// Hit regions for clickable action bar buttons.
#[derive(Debug, Clone, Copy, Default)]
pub struct GitActionRects {
    pub add_all: Rect,
    pub commit: Rect,
}

/// Git sidebar panel state.
#[derive(Debug, Clone)]
pub struct GitState {
    pub panel_id: Option<ViewId>,
    pub entries: Vec<GitStatusEntry>,
    pub selection: Option<GitSelection>,
    pub scroll: usize,
    pub branch: Option<String>,
    pub loading: bool,
    pub error: Option<String>,
    pub action_rects: GitActionRects,
}

impl GitState {
    pub fn new() -> Self {
        Self {
            panel_id: None,
            entries: Vec::new(),
            selection: None,
            scroll: 0,
            branch: None,
            loading: false,
            error: None,
            action_rects: GitActionRects::default(),
        }
    }

    pub fn is_open(&self) -> bool {
        self.panel_id.is_some()
    }

    pub fn unstaged(&self) -> impl Iterator<Item = (usize, &GitStatusEntry)> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.section == StagingSection::Unstaged)
    }

    pub fn staged(&self) -> impl Iterator<Item = (usize, &GitStatusEntry)> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.section == StagingSection::Staged)
    }

    pub fn selected_entry(&self) -> Option<&GitStatusEntry> {
        let selection = self.selection?;
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.section == selection.section)
            .nth(selection.index)
            .map(|(_, entry)| entry)
    }
}

impl Default for GitState {
    fn default() -> Self {
        Self::new()
    }
}
