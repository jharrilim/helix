//! Plan review panel state stored on the editor.

use crate::ViewId;

/// Active plan review payload from `cursor/create_plan`.
#[derive(Debug, Clone)]
pub struct PlanReview {
    pub request_id: u64,
    pub tool_call_id: String,
    pub name: Option<String>,
    pub markdown: String,
}

/// Keyboard focus within the plan panel footer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlanFooterFocus {
    #[default]
    Accept,
    Reject,
    Cancel,
    QuestionOption(usize),
}

/// Runtime UI state for the plan review panel.
#[derive(Debug, Default)]
pub struct PlanState {
    /// Tree node id of the plan panel, if open.
    pub panel_id: Option<ViewId>,
    /// Editor container temporarily removed from the root while the plan panel is open.
    pub stashed_editor_root: Option<ViewId>,
    /// Active plan review request, if any.
    pub review: Option<PlanReview>,
    pub footer_focus: PlanFooterFocus,
    pub scroll: usize,
    /// Title shown when the panel is open without plan markdown (question-only).
    pub title: Option<String>,
}

impl PlanState {
    pub fn is_open(&self) -> bool {
        self.panel_id.is_some()
    }

    pub fn clear_review(&mut self) {
        self.review = None;
        self.title = None;
        self.scroll = 0;
        self.footer_focus = PlanFooterFocus::Accept;
    }
}
