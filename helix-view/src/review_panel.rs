//! Code review sidebar panel state.

use crate::{graphics::Rect, ViewId};
use helix_review::ReviewListEntry;

/// Selected row in the review panel list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewPanelSelection {
    Review(usize),
    Comment(usize),
}

/// Hit regions for the review panel action bar.
#[derive(Debug, Clone, Copy, Default)]
pub struct ReviewPanelActionRects {
    pub new_review: Rect,
    pub submit: Rect,
    pub delete: Rect,
}

/// Review sidebar panel state stored on the editor.
#[derive(Debug, Clone, Default)]
pub struct ReviewPanelState {
    pub panel_id: Option<ViewId>,
    pub entries: Vec<ReviewListEntry>,
    pub selection: Option<ReviewPanelSelection>,
    pub scroll: usize,
    pub action_rects: ReviewPanelActionRects,
}

impl ReviewPanelState {
    pub fn is_open(&self) -> bool {
        self.panel_id.is_some()
    }
}
