use helix_core::fold::FoldState;

use crate::view::ViewPosition;

#[derive(Debug, Default)]
pub struct ViewData {
    view_position: ViewPosition,
    pub folds: FoldState,
}

impl ViewData {
    pub fn folds(&self) -> &FoldState {
        &self.folds
    }

    pub fn folds_mut(&mut self) -> &mut FoldState {
        &mut self.folds
    }

    pub(crate) fn view_position(&self) -> ViewPosition {
        self.view_position
    }

    pub(crate) fn view_position_mut(&mut self) -> &mut ViewPosition {
        &mut self.view_position
    }
}
