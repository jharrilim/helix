//! Focus targets for split-tree leaves.

use crate::{tree::LeafKind, ViewId};

/// Identifies which split-tree leaf has focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusTarget {
    Document(ViewId),
    Agent(ViewId),
    Git(ViewId),
    Terminal(ViewId),
}

impl FocusTarget {
    pub fn id(&self) -> ViewId {
        match self {
            Self::Document(id)
            | Self::Agent(id)
            | Self::Git(id)
            | Self::Terminal(id) => *id,
        }
    }

    pub fn from_kind(id: ViewId, kind: LeafKind) -> Self {
        match kind {
            LeafKind::View => Self::Document(id),
            LeafKind::AgentPanel => Self::Agent(id),
            LeafKind::GitPanel => Self::Git(id),
            LeafKind::TerminalPanel => Self::Terminal(id),
        }
    }

    pub fn leaf_kind(&self) -> LeafKind {
        match self {
            Self::Document(_) => LeafKind::View,
            Self::Agent(_) => LeafKind::AgentPanel,
            Self::Git(_) => LeafKind::GitPanel,
            Self::Terminal(_) => LeafKind::TerminalPanel,
        }
    }

    pub fn is_document(&self) -> bool {
        matches!(self, Self::Document(_))
    }
}
