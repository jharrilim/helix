//! Split tree for editor views and auxiliary panel leaves.
//!
//! `tree.focus` may point at a document [`View`] or an auxiliary panel leaf
//! (agent, git, terminal). Use [`Tree::try_focused_view`] or
//! [`Tree::focused_kind`] when the focused node might not be a view. Do not
//! call [`Tree::get`] / [`Tree::get_mut`] on panel IDs.

use crate::{graphics::Rect, View, ViewId};
use slotmap::SlotMap;

/// Kind of leaf node in the split tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LeafKind {
    View,
    AgentPanel,
    GitPanel,
    TerminalPanel,
}

/// Agent chat panel leaf in the split tree.
#[derive(Debug)]
pub struct AgentPanel {
    pub id: ViewId,
    pub area: Rect,
}

/// Git status panel leaf in the split tree.
#[derive(Debug)]
pub struct GitPanel {
    pub id: ViewId,
    pub area: Rect,
}

/// Integrated terminal panel leaf in the split tree.
#[derive(Debug)]
pub struct TerminalPanel {
    pub id: ViewId,
    pub area: Rect,
    pub session_id: String,
}

// the dimensions are recomputed on window resize/tree change.
//
#[derive(Debug)]
pub struct Tree {
    root: ViewId,
    // (container, index inside the container)
    pub focus: ViewId,
    // fullscreen: bool,
    area: Rect,

    nodes: SlotMap<ViewId, Node>,

    // used for traversals
    stack: Vec<(ViewId, Rect)>,
}

#[derive(Debug)]
pub struct Node {
    parent: ViewId,
    content: Content,
}

#[derive(Debug)]
pub enum Content {
    View(Box<View>),
    AgentPanel(AgentPanel),
    GitPanel(GitPanel),
    TerminalPanel(TerminalPanel),
    Container(Box<Container>),
}

impl Content {
    /// Returns the leaf kind when this node is a leaf, not a container.
    pub fn leaf_kind(&self) -> Option<LeafKind> {
        match self {
            Self::View(_) => Some(LeafKind::View),
            Self::AgentPanel(_) => Some(LeafKind::AgentPanel),
            Self::GitPanel(_) => Some(LeafKind::GitPanel),
            Self::TerminalPanel(_) => Some(LeafKind::TerminalPanel),
            Self::Container(_) => None,
        }
    }

    /// Returns the screen area for a leaf node.
    pub fn leaf_area(&self) -> Option<Rect> {
        match self {
            Self::View(view) => Some(view.area),
            Self::AgentPanel(panel) => Some(panel.area),
            Self::GitPanel(panel) => Some(panel.area),
            Self::TerminalPanel(panel) => Some(panel.area),
            Self::Container(_) => None,
        }
    }

    /// Sets the screen area for a leaf node.
    pub fn set_leaf_area(&mut self, area: Rect) {
        match self {
            Self::View(view) => view.area = area,
            Self::AgentPanel(panel) => panel.area = area,
            Self::GitPanel(panel) => panel.area = area,
            Self::TerminalPanel(panel) => panel.area = area,
            Self::Container(_) => {}
        }
    }

    fn leaf_id(&self) -> Option<ViewId> {
        match self {
            Self::View(view) => Some(view.id),
            Self::AgentPanel(panel) => Some(panel.id),
            Self::GitPanel(panel) => Some(panel.id),
            Self::TerminalPanel(panel) => Some(panel.id),
            Self::Container(_) => None,
        }
    }
}

impl Node {
    pub fn container(layout: Layout) -> Self {
        Self {
            parent: ViewId::default(),
            content: Content::Container(Box::new(Container::new(layout))),
        }
    }

    pub fn view(view: View) -> Self {
        Self {
            parent: ViewId::default(),
            content: Content::View(Box::new(view)),
        }
    }

    pub fn agent_panel(panel: AgentPanel) -> Self {
        Self {
            parent: ViewId::default(),
            content: Content::AgentPanel(panel),
        }
    }

    pub fn git_panel(panel: GitPanel) -> Self {
        Self {
            parent: ViewId::default(),
            content: Content::GitPanel(panel),
        }
    }

    pub fn terminal_panel(panel: TerminalPanel) -> Self {
        Self {
            parent: ViewId::default(),
            content: Content::TerminalPanel(panel),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Horizontal,
    Vertical,
    // could explore stacked/tabbed
}

#[derive(Debug, Clone, Copy)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

/// Axis along which a split divider is dragged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeAxis {
    /// Vertical split (panes side-by-side); drag adjusts column boundary.
    Vertical,
    /// Horizontal split (panes stacked); drag adjusts row boundary.
    Horizontal,
}

/// Identifies a draggable divider between two siblings in a container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResizeHandle {
    pub container_id: ViewId,
    pub divider_index: usize,
}

/// Minimum width or height in cells for any leaf after resizing.
const MIN_LEAF_SIZE: u16 = 10;

#[derive(Debug)]
pub struct Container {
    layout: Layout,
    children: Vec<ViewId>,
    area: Rect,
    /// Flex weights for each child; always `len == children.len()`.
    weights: Vec<f32>,
}

impl Container {
    pub fn new(layout: Layout) -> Self {
        Self {
            layout,
            children: Vec::new(),
            area: Rect::default(),
            weights: Vec::new(),
        }
    }

    fn ensure_weights(&mut self) {
        if self.weights.len() != self.children.len() {
            self.weights = vec![1.0; self.children.len()];
        }
    }

    fn insert_child(&mut self, pos: usize, child: ViewId) {
        self.children.insert(pos, child);
        self.ensure_weights();
        self.weights.insert(pos, 1.0);
    }

    fn push_child(&mut self, child: ViewId) {
        self.children.push(child);
        self.ensure_weights();
        self.weights.push(1.0);
    }

    fn remove_child(&mut self, pos: usize) {
        self.children.remove(pos);
        if pos < self.weights.len() {
            self.weights.remove(pos);
        }
    }
}

impl Default for Container {
    fn default() -> Self {
        Self::new(Layout::Vertical)
    }
}

impl Tree {
    pub fn new(area: Rect) -> Self {
        let root = Node::container(Layout::Vertical);

        let mut nodes = SlotMap::with_key();
        let root = nodes.insert(root);

        // root is it's own parent
        nodes[root].parent = root;

        Self {
            root,
            focus: root,
            // fullscreen: false,
            area,
            nodes,
            stack: Vec::new(),
        }
    }

    pub fn insert(&mut self, view: View) -> ViewId {
        let focus = self.focus;
        let parent = self.nodes[focus].parent;
        let mut node = Node::view(view);
        node.parent = parent;
        let node = self.nodes.insert(node);
        self.get_mut(node).id = node;

        let container = match &mut self.nodes[parent] {
            Node {
                content: Content::Container(container),
                ..
            } => container,
            _ => unreachable!(),
        };

        // insert node after the current item if there is children already
        let pos = if container.children.is_empty() {
            0
        } else {
            let pos = container
                .children
                .iter()
                .position(|&child| child == focus)
                .unwrap();
            pos + 1
        };

        container.insert_child(pos, node);
        // focus the new node
        self.focus = node;

        // recalculate all the sizes
        self.recalculate();

        node
    }

    pub fn split(&mut self, view: View, layout: Layout) -> ViewId {
        let focus = self.focus;
        let parent = self.nodes[focus].parent;

        let node = Node::view(view);
        let node = self.nodes.insert(node);
        self.get_mut(node).id = node;

        let container = match &mut self.nodes[parent] {
            Node {
                content: Content::Container(container),
                ..
            } => container,
            _ => unreachable!(),
        };
        if container.layout == layout {
            // insert node after the current item if there is children already
            let pos = if container.children.is_empty() {
                0
            } else {
                let pos = container
                    .children
                    .iter()
                    .position(|&child| child == focus)
                    .unwrap();
                pos + 1
            };
            container.insert_child(pos, node);
            self.nodes[node].parent = parent;
        } else {
            let mut split = Node::container(layout);
            split.parent = parent;
            let split = self.nodes.insert(split);

            let container = match &mut self.nodes[split] {
                Node {
                    content: Content::Container(container),
                    ..
                } => container,
                _ => unreachable!(),
            };
            container.push_child(focus);
            container.push_child(node);
            self.nodes[focus].parent = split;
            self.nodes[node].parent = split;

            let container = match &mut self.nodes[parent] {
                Node {
                    content: Content::Container(container),
                    ..
                } => container,
                _ => unreachable!(),
            };

            let pos = container
                .children
                .iter()
                .position(|&child| child == focus)
                .unwrap();

            // replace focus on parent with split
            container.children[pos] = split;
        }

        // focus the new node
        self.focus = node;

        // recalculate all the sizes
        self.recalculate();

        node
    }

    pub fn split_agent_panel(&mut self, layout: Layout) -> ViewId {
        let focus = self.focus;
        let parent = self.nodes[focus].parent;

        let node = Node::agent_panel(AgentPanel {
            id: ViewId::default(),
            area: Rect::default(),
        });
        let node = self.nodes.insert(node);
        if let Node {
            content: Content::AgentPanel(panel),
            ..
        } = &mut self.nodes[node]
        {
            panel.id = node;
        }

        let container = match &mut self.nodes[parent] {
            Node {
                content: Content::Container(container),
                ..
            } => container,
            _ => unreachable!(),
        };
        if container.layout == layout {
            let pos = if container.children.is_empty() {
                0
            } else {
                container
                    .children
                    .iter()
                    .position(|&child| child == focus)
                    .unwrap()
                    + 1
            };
            container.insert_child(pos, node);
            self.nodes[node].parent = parent;
        } else {
            let mut split = Node::container(layout);
            split.parent = parent;
            let split = self.nodes.insert(split);

            let container = match &mut self.nodes[split] {
                Node {
                    content: Content::Container(container),
                    ..
                } => container,
                _ => unreachable!(),
            };
            container.push_child(focus);
            container.push_child(node);
            self.nodes[focus].parent = split;
            self.nodes[node].parent = split;

            let container = match &mut self.nodes[parent] {
                Node {
                    content: Content::Container(container),
                    ..
                } => container,
                _ => unreachable!(),
            };

            let pos = container
                .children
                .iter()
                .position(|&child| child == focus)
                .unwrap();

            container.children[pos] = split;
        }

        self.focus = node;
        self.recalculate();
        node
    }

    /// Insert a git panel as the leftmost full-height column at the tree root.
    pub fn split_git_panel(&mut self, _layout: Layout) -> ViewId {
        let node = Node::git_panel(GitPanel {
            id: ViewId::default(),
            area: Rect::default(),
        });
        let node = self.nodes.insert(node);
        if let Node {
            content: Content::GitPanel(panel),
            ..
        } = &mut self.nodes[node]
        {
            panel.id = node;
        }

        let root = self.root;
        let root_layout = match &self.nodes[root].content {
            Content::Container(container) => container.layout,
            _ => unreachable!(),
        };

        match root_layout {
            Layout::Vertical => {
                let container = self.container_mut(root);
                container.insert_child(0, node);
                self.nodes[node].parent = root;
            }
            Layout::Horizontal => {
                let (children, weights) = {
                    let container = self.container_mut(root);
                    (
                        std::mem::take(&mut container.children),
                        std::mem::take(&mut container.weights),
                    )
                };

                let mut inner = Node::container(Layout::Horizontal);
                inner.parent = root;
                let inner_id = self.nodes.insert(inner);

                {
                    let inner_container = self.container_mut(inner_id);
                    inner_container.children = children;
                    inner_container.weights = if weights.len() == inner_container.children.len() {
                        weights
                    } else {
                        vec![1.0; inner_container.children.len()]
                    };
                }
                for child in self.container_mut(inner_id).children.clone() {
                    self.nodes[child].parent = inner_id;
                }

                let container = self.container_mut(root);
                container.layout = Layout::Vertical;
                container.children = vec![node, inner_id];
                container.weights = vec![1.0, 1.0];
                self.nodes[node].parent = root;
            }
        }

        self.focus = node;
        self.recalculate();
        node
    }

    pub fn split_terminal_panel(&mut self, layout: Layout, session_id: String) -> ViewId {
        let focus = self.focus;
        let parent = self.nodes[focus].parent;

        let node = Node::terminal_panel(TerminalPanel {
            id: ViewId::default(),
            area: Rect::default(),
            session_id,
        });
        let node = self.nodes.insert(node);
        if let Node {
            content: Content::TerminalPanel(panel),
            ..
        } = &mut self.nodes[node]
        {
            panel.id = node;
        }

        let container = match &mut self.nodes[parent] {
            Node {
                content: Content::Container(container),
                ..
            } => container,
            _ => unreachable!(),
        };
        if container.layout == layout {
            let pos = if container.children.is_empty() {
                0
            } else {
                container
                    .children
                    .iter()
                    .position(|&child| child == focus)
                    .unwrap()
                    + 1
            };
            container.insert_child(pos, node);
            self.nodes[node].parent = parent;
        } else {
            let mut split = Node::container(layout);
            split.parent = parent;
            let split = self.nodes.insert(split);

            let container = match &mut self.nodes[split] {
                Node {
                    content: Content::Container(container),
                    ..
                } => container,
                _ => unreachable!(),
            };
            container.push_child(focus);
            container.push_child(node);
            self.nodes[focus].parent = split;
            self.nodes[node].parent = split;

            let container = match &mut self.nodes[parent] {
                Node {
                    content: Content::Container(container),
                    ..
                } => container,
                _ => unreachable!(),
            };

            let pos = container
                .children
                .iter()
                .position(|&child| child == focus)
                .unwrap();

            container.children[pos] = split;
        }

        self.focus = node;
        self.recalculate();
        node
    }

    /// Get a mutable reference to a [Container] by index.
    /// # Panics
    /// Panics if `index` is not in self.nodes, or if the node's content is not a [Content::Container].
    fn container_mut(&mut self, index: ViewId) -> &mut Container {
        match &mut self.nodes[index] {
            Node {
                content: Content::Container(container),
                ..
            } => container,
            _ => unreachable!(),
        }
    }

    fn remove_or_replace(&mut self, child: ViewId, replacement: Option<ViewId>) {
        let parent = self.nodes[child].parent;

        self.nodes.remove(child);

        let container = self.container_mut(parent);
        let pos = container
            .children
            .iter()
            .position(|&item| item == child)
            .unwrap();

        if let Some(new) = replacement {
            container.children[pos] = new;
            self.nodes[new].parent = parent;
        } else {
            container.remove_child(pos);
        }
    }

    pub fn remove(&mut self, index: ViewId) {
        if self.focus == index {
            // focus on something else
            self.focus = self.prev();
        }

        let parent = self.nodes[index].parent;
        let parent_is_root = parent == self.root;

        self.remove_or_replace(index, None);

        let parent_container = self.container_mut(parent);
        if parent_container.children.len() == 1 && !parent_is_root {
            // Lets merge the only child back to its grandparent so that Views
            // are equally spaced.
            let sibling = parent_container.children.pop().unwrap();
            self.remove_or_replace(parent, Some(sibling));
        }

        self.recalculate()
    }

    pub fn views(&self) -> impl Iterator<Item = (&View, bool)> {
        let focus = self.focus;
        self.nodes.iter().filter_map(move |(key, node)| match node {
            Node {
                content: Content::View(view),
                ..
            } => Some((view.as_ref(), focus == key)),
            _ => None,
        })
    }

    pub fn views_mut(&mut self) -> impl Iterator<Item = (&mut View, bool)> {
        let focus = self.focus;
        self.nodes
            .iter_mut()
            .filter_map(move |(key, node)| match node {
                Node {
                    content: Content::View(view),
                    ..
                } => Some((view.as_mut(), focus == key)),
                _ => None,
            })
    }

    pub fn is_agent_panel(&self, index: ViewId) -> bool {
        matches!(
            self.nodes.get(index),
            Some(Node {
                content: Content::AgentPanel(_),
                ..
            })
        )
    }

    pub fn agent_panel(&self, index: ViewId) -> Option<&AgentPanel> {
        match self.nodes.get(index) {
            Some(Node {
                content: Content::AgentPanel(panel),
                ..
            }) => Some(panel),
            _ => None,
        }
    }

    pub fn agent_panel_mut(&mut self, index: ViewId) -> Option<&mut AgentPanel> {
        match self.nodes.get_mut(index) {
            Some(Node {
                content: Content::AgentPanel(panel),
                ..
            }) => Some(panel),
            _ => None,
        }
    }

    pub fn agent_panels(&self) -> impl Iterator<Item = (&AgentPanel, bool)> {
        let focus = self.focus;
        self.nodes.iter().filter_map(move |(key, node)| match node {
            Node {
                content: Content::AgentPanel(panel),
                ..
            } => Some((panel, focus == key)),
            _ => None,
        })
    }

    pub fn is_git_panel(&self, index: ViewId) -> bool {
        matches!(
            self.nodes.get(index),
            Some(Node {
                content: Content::GitPanel(_),
                ..
            })
        )
    }

    pub fn git_panel(&self, index: ViewId) -> Option<&GitPanel> {
        match self.nodes.get(index) {
            Some(Node {
                content: Content::GitPanel(panel),
                ..
            }) => Some(panel),
            _ => None,
        }
    }

    pub fn git_panel_mut(&mut self, index: ViewId) -> Option<&mut GitPanel> {
        match self.nodes.get_mut(index) {
            Some(Node {
                content: Content::GitPanel(panel),
                ..
            }) => Some(panel),
            _ => None,
        }
    }

    pub fn git_panels(&self) -> impl Iterator<Item = (&GitPanel, bool)> {
        let focus = self.focus;
        self.nodes.iter().filter_map(move |(key, node)| match node {
            Node {
                content: Content::GitPanel(panel),
                ..
            } => Some((panel, focus == key)),
            _ => None,
        })
    }

    pub fn is_terminal_panel(&self, index: ViewId) -> bool {
        matches!(
            self.nodes.get(index),
            Some(Node {
                content: Content::TerminalPanel(_),
                ..
            })
        )
    }

    pub fn terminal_panel(&self, index: ViewId) -> Option<&TerminalPanel> {
        match self.nodes.get(index) {
            Some(Node {
                content: Content::TerminalPanel(panel),
                ..
            }) => Some(panel),
            _ => None,
        }
    }

    pub fn terminal_panel_mut(&mut self, index: ViewId) -> Option<&mut TerminalPanel> {
        match self.nodes.get_mut(index) {
            Some(Node {
                content: Content::TerminalPanel(panel),
                ..
            }) => Some(panel),
            _ => None,
        }
    }

    pub fn terminal_panels(&self) -> impl Iterator<Item = (&TerminalPanel, bool)> {
        let focus = self.focus;
        self.nodes.iter().filter_map(move |(key, node)| match node {
            Node {
                content: Content::TerminalPanel(panel),
                ..
            } => Some((panel, focus == key)),
            _ => None,
        })
    }

    /// Get reference to a [View] by index.
    /// # Panics
    ///
    /// Panics if `index` is not in self.nodes, or if the node's content is not [Content::View]. This can be checked with [Self::contains].
    pub fn get(&self, index: ViewId) -> &View {
        self.try_get(index).unwrap()
    }

    /// Try to get reference to a [View] by index. Returns `None` if node content is not a [`Content::View`].
    ///
    /// Does not panic if the view does not exists anymore.
    pub fn try_get(&self, index: ViewId) -> Option<&View> {
        match self.nodes.get(index) {
            Some(Node {
                content: Content::View(view),
                ..
            }) => Some(view),
            _ => None,
        }
    }

    /// Returns the kind of leaf at `id`, if any.
    pub fn leaf_kind(&self, id: ViewId) -> Option<LeafKind> {
        self.nodes.get(id)?.content.leaf_kind()
    }

    /// Returns the kind of the currently focused leaf, if any.
    pub fn focused_kind(&self) -> Option<LeafKind> {
        self.leaf_kind(self.focus)
    }

    /// Returns the focused document view, if focus is on a view leaf.
    pub fn try_focused_view(&self) -> Option<&View> {
        self.try_get(self.focus)
    }

    /// Get a mutable reference to a [View] by index.
    /// # Panics
    ///
    /// Panics if `index` is not in self.nodes, or if the node's content is not [Content::View]. This can be checked with [Self::contains].
    pub fn get_mut(&mut self, index: ViewId) -> &mut View {
        match &mut self.nodes[index] {
            Node {
                content: Content::View(view),
                ..
            } => view,
            _ => unreachable!(),
        }
    }

    /// Try to get a mutable reference to a [View] by index.
    pub fn try_get_mut(&mut self, index: ViewId) -> Option<&mut View> {
        match &mut self.nodes.get_mut(index)?.content {
            Content::View(view) => Some(view),
            _ => None,
        }
    }

    /// Check if tree contains a [Node] with a given index.
    pub fn contains(&self, index: ViewId) -> bool {
        self.nodes.contains_key(index)
    }

    pub fn is_empty(&self) -> bool {
        match &self.nodes[self.root] {
            Node {
                content: Content::Container(container),
                ..
            } => container.children.is_empty(),
            _ => unreachable!(),
        }
    }

    pub fn resize(&mut self, area: Rect) -> bool {
        if self.area != area {
            self.area = area;
            self.recalculate();
            return true;
        }
        false
    }

    pub fn recalculate(&mut self) {
        if self.is_empty() {
            // There are no more views, so the tree should focus itself again.
            self.focus = self.root;

            return;
        }

        self.stack.push((self.root, self.area));

        while let Some((key, area)) = self.stack.pop() {
            let node = &mut self.nodes[key];

            if node.content.leaf_kind().is_some() {
                node.content.set_leaf_area(area);
            } else if let Content::Container(container) = &mut node.content {
                    container.area = area;
                    container.ensure_weights();

                    match container.layout {
                        Layout::Horizontal => {
                            let len = container.children.len();
                            let total_weight: f32 = container.weights.iter().sum();
                            let total_weight = if total_weight > 0.0 {
                                total_weight
                            } else {
                                len as f32
                            };

                            let mut child_y = area.y;

                            for (i, child) in container.children.iter().enumerate() {
                                let mut child_height = if i == len - 1 {
                                    container.area.y + container.area.height - child_y
                                } else {
                                    let fraction = container.weights[i] / total_weight;
                                    (area.height as f32 * fraction).floor() as u16
                                };
                                child_height = child_height.max(1);

                                let child_area = Rect::new(
                                    container.area.x,
                                    child_y,
                                    container.area.width,
                                    child_height,
                                );
                                child_y = child_y.saturating_add(child_height);

                                self.stack.push((*child, child_area));
                            }
                        }
                        Layout::Vertical => {
                            let len = container.children.len();
                            let len_u16 = len as u16;

                            let inner_gap = 1u16;
                            let total_gap = inner_gap * len_u16.saturating_sub(2);
                            let used_area = area.width.saturating_sub(total_gap);

                            let total_weight: f32 = container.weights.iter().sum();
                            let total_weight = if total_weight > 0.0 {
                                total_weight
                            } else {
                                len as f32
                            };

                            let mut child_x = area.x;

                            for (i, child) in container.children.iter().enumerate() {
                                let mut child_width = if i == len - 1 {
                                    container.area.x + container.area.width - child_x
                                } else {
                                    let fraction = container.weights[i] / total_weight;
                                    (used_area as f32 * fraction).floor() as u16
                                };
                                child_width = child_width.max(1);

                                let child_area = Rect::new(
                                    child_x,
                                    container.area.y,
                                    child_width,
                                    container.area.height,
                                );
                                child_x = child_x.saturating_add(child_width + inner_gap);

                                self.stack.push((*child, child_area));
                            }
                        }
                    }
            }
        }
    }

    pub fn traverse(&self) -> Traverse<'_> {
        Traverse::new(self)
    }

    pub fn leaves(&self) -> LeafTraverse<'_> {
        LeafTraverse::new(self)
    }

    // Finds the split in the given direction if it exists
    pub fn find_split_in_direction(&self, id: ViewId, direction: Direction) -> Option<ViewId> {
        let parent = self.nodes[id].parent;
        // Base case, we found the root of the tree
        if parent == id {
            return None;
        }
        // Parent must always be a container
        let parent_container = match &self.nodes[parent].content {
            Content::Container(container) => container,
            Content::View(_) | Content::AgentPanel(_) | Content::GitPanel(_) | Content::TerminalPanel(_) => unreachable!(),
        };

        match (direction, parent_container.layout) {
            (Direction::Up, Layout::Vertical)
            | (Direction::Left, Layout::Horizontal)
            | (Direction::Right, Layout::Horizontal)
            | (Direction::Down, Layout::Vertical) => {
                // The desired direction of movement is not possible within
                // the parent container so the search must continue closer to
                // the root of the split tree.
                self.find_split_in_direction(parent, direction)
            }
            (Direction::Up, Layout::Horizontal)
            | (Direction::Down, Layout::Horizontal)
            | (Direction::Left, Layout::Vertical)
            | (Direction::Right, Layout::Vertical) => {
                // It's possible to move in the desired direction within
                // the parent container so an attempt is made to find the
                // correct child.
                match self.find_child(id, &parent_container.children, direction) {
                    // Child is found, search is ended
                    Some(id) => Some(id),
                    // A child is not found. This could be because of either two scenarios
                    // 1. Its not possible to move in the desired direction, and search should end
                    // 2. A layout like the following with focus at X and desired direction Right
                    // | _ | x |   |
                    // | _ _ _ |   |
                    // | _ _ _ |   |
                    // The container containing X ends at X so no rightward movement is possible
                    // however there still exists another view/container to the right that hasn't
                    // been explored. Thus another search is done here in the parent container
                    // before concluding it's not possible to move in the desired direction.
                    None => self.find_split_in_direction(parent, direction),
                }
            }
        }
    }

    fn find_child(&self, id: ViewId, children: &[ViewId], direction: Direction) -> Option<ViewId> {
        let mut child_id = match direction {
            // index wise in the child list the Up and Left represents a -1
            // thus reversed iterator.
            Direction::Up | Direction::Left => children
                .iter()
                .rev()
                .skip_while(|i| **i != id)
                .copied()
                .nth(1)?,
            // Down and Right => +1 index wise in the child list
            Direction::Down | Direction::Right => {
                children.iter().skip_while(|i| **i != id).copied().nth(1)?
            }
        };
        let (current_x, current_y) = {
            let area = self.nodes[self.focus].content.leaf_area()?;
            (area.left(), area.top())
        };

        // If the child is a container the search finds the closest container child
        // visually based on screen location.
        while let Content::Container(container) = &self.nodes[child_id].content {
            match (direction, container.layout) {
                (_, Layout::Vertical) => {
                    // find closest split based on x because y is irrelevant
                    // in a vertical container (and already correct based on previous search)
                    child_id = *container.children.iter().min_by_key(|id| {
                        let x = self.nodes[**id]
                            .content
                            .leaf_area()
                            .map(|area| area.left())
                            .unwrap_or_else(|| {
                                match &self.nodes[**id].content {
                                    Content::Container(container) => container.area.left(),
                                    _ => 0,
                                }
                            });
                        (current_x as i16 - x as i16).abs()
                    })?;
                }
                (_, Layout::Horizontal) => {
                    // find closest split based on y because x is irrelevant
                    // in a horizontal container (and already correct based on previous search)
                    child_id = *container.children.iter().min_by_key(|id| {
                        let y = self.nodes[**id]
                            .content
                            .leaf_area()
                            .map(|area| area.top())
                            .unwrap_or_else(|| {
                                match &self.nodes[**id].content {
                                    Content::Container(container) => container.area.top(),
                                    _ => 0,
                                }
                            });
                        (current_y as i16 - y as i16).abs()
                    })?;
                }
            }
        }
        Some(child_id)
    }

    pub fn prev(&self) -> ViewId {
        let leaves: Vec<ViewId> = self.leaves().map(|(id, _)| id).collect();
        if let Some(pos) = leaves.iter().position(|&id| id == self.focus) {
            if pos > 0 {
                return leaves[pos - 1];
            }
        }
        *leaves.last().unwrap_or(&self.focus)
    }

    pub fn next(&self) -> ViewId {
        let leaves: Vec<ViewId> = self.leaves().map(|(id, _)| id).collect();
        if let Some(pos) = leaves.iter().position(|&id| id == self.focus) {
            if pos + 1 < leaves.len() {
                return leaves[pos + 1];
            }
        }
        *leaves.first().unwrap_or(&self.focus)
    }

    pub fn transpose(&mut self) {
        let focus = self.focus;
        let parent = self.nodes[focus].parent;
        if let Content::Container(container) = &mut self.nodes[parent].content {
            container.layout = match container.layout {
                Layout::Vertical => Layout::Horizontal,
                Layout::Horizontal => Layout::Vertical,
            };
            self.recalculate();
        }
    }

    pub fn swap_split_in_direction(&mut self, direction: Direction) -> Option<()> {
        let focus = self.focus;
        let target = self.find_split_in_direction(focus, direction)?;
        if focus == target {
            return None;
        }
        let focus_parent = self.nodes[focus].parent;
        let target_parent = self.nodes[target].parent;

        if focus_parent == target_parent {
            let parent_id = focus_parent;
            let [parent, focus_node, target_node] =
                self.nodes.get_disjoint_mut([parent_id, focus, target])?;
            let Content::Container(parent) = &mut parent.content else {
                return None;
            };
            if focus_node.content.leaf_kind().is_none() || target_node.content.leaf_kind().is_none() {
                return None;
            }
            Self::swap_leaf_siblings(parent, &mut focus_node.content, &mut target_node.content, focus, target)
        } else {
            let [focus_parent_node, target_parent_node, focus_node, target_node] =
                self.nodes
                    .get_disjoint_mut([focus_parent, target_parent, focus, target])?;
            let (Content::Container(focus_parent), Content::Container(target_parent)) =
                (&mut focus_parent_node.content, &mut target_parent_node.content)
            else {
                return None;
            };
            if focus_node.content.leaf_kind().is_none() || target_node.content.leaf_kind().is_none() {
                return None;
            }
            let focus_id = focus_node.content.leaf_id()?;
            let target_id = target_node.content.leaf_id()?;
            let focus_pos = focus_parent
                .children
                .iter()
                .position(|id| *id == focus)?;
            let target_pos = target_parent
                .children
                .iter()
                .position(|id| *id == target)?;
            std::mem::swap(
                &mut focus_parent.children[focus_pos],
                &mut target_parent.children[target_pos],
            );
            if focus_parent.weights.len() == focus_parent.children.len()
                && target_parent.weights.len() == target_parent.children.len()
            {
                std::mem::swap(
                    &mut focus_parent.weights[focus_pos],
                    &mut target_parent.weights[target_pos],
                );
            }
            std::mem::swap(&mut focus_node.parent, &mut target_node.parent);
            let focus_area = focus_node.content.leaf_area()?;
            let target_area = target_node.content.leaf_area()?;
            focus_node.content.set_leaf_area(target_area);
            target_node.content.set_leaf_area(focus_area);
            let _ = (focus_id, target_id);
            Some(())
        }
    }

    fn swap_leaf_siblings(
        parent: &mut Container,
        focus_content: &mut Content,
        target_content: &mut Content,
        focus: ViewId,
        target: ViewId,
    ) -> Option<()> {
        let focus_pos = parent.children.iter().position(|id| *id == focus)?;
        let target_pos = parent.children.iter().position(|id| *id == target)?;
        parent.children[focus_pos] = target;
        parent.children[target_pos] = focus;
        parent.weights.swap(focus_pos, target_pos);
        let focus_area = focus_content.leaf_area()?;
        let target_area = target_content.leaf_area()?;
        focus_content.set_leaf_area(target_area);
        target_content.set_leaf_area(focus_area);
        Some(())
    }

    pub fn area(&self) -> Rect {
        self.area
    }

    fn node_area(&self, id: ViewId) -> Rect {
        match &self.nodes[id].content {
            Content::Container(container) => container.area,
            content => content.leaf_area().unwrap_or_default(),
        }
    }

    /// Sets the weight ratio for a leaf among its two siblings in a container.
    pub fn set_leaf_weight_fraction(&mut self, leaf_id: ViewId, fraction: f32) {
        let parent = self.nodes[leaf_id].parent;
        let Content::Container(container) = &mut self.nodes[parent].content else {
            return;
        };
        if container.children.len() != 2 {
            return;
        }
        container.ensure_weights();
        let Some(pos) = container.children.iter().position(|&id| id == leaf_id) else {
            return;
        };
        let sibling = 1 - pos;
        let fraction = fraction.clamp(0.1, 0.9);
        container.weights[pos] = fraction;
        container.weights[sibling] = 1.0 - fraction;
        self.recalculate();
    }

    /// Returns a resize handle at the given screen coordinates, if any interior divider matches.
    pub fn resize_handle_at(&self, row: u16, column: u16) -> Option<(ResizeHandle, ResizeAxis)> {
        let mut stack = vec![self.root];
        while let Some(id) = stack.pop() {
            let Content::Container(container) = &self.nodes[id].content else {
                continue;
            };

            if container.children.len() >= 2 {
                for divider_index in 0..container.children.len() - 1 {
                    let left = self.node_area(container.children[divider_index]);
                    let right = self.node_area(container.children[divider_index + 1]);

                    let handle = match container.layout {
                        Layout::Vertical => {
                            let boundary = left.right();
                            if row < left.y || row >= left.bottom() {
                                continue;
                            }
                            if column != boundary.saturating_sub(1) && column != boundary {
                                continue;
                            }
                            if boundary <= self.area.x || boundary >= self.area.right() {
                                continue;
                            }
                            (
                                ResizeHandle {
                                    container_id: id,
                                    divider_index,
                                },
                                ResizeAxis::Vertical,
                            )
                        }
                        Layout::Horizontal => {
                            let boundary = left.bottom();
                            if column < left.x || column >= left.right() {
                                continue;
                            }
                            if row != boundary.saturating_sub(1) && row != boundary {
                                continue;
                            }
                            if boundary <= self.area.y || boundary >= self.area.bottom() {
                                continue;
                            }
                            (
                                ResizeHandle {
                                    container_id: id,
                                    divider_index,
                                },
                                ResizeAxis::Horizontal,
                            )
                        }
                    };

                    let _ = right;
                    return Some(handle);
                }
            }

            stack.extend(container.children.iter().copied());
        }
        None
    }

    /// Adjusts the split at `handle` by `delta` cells along the resize axis.
    pub fn adjust_resize(&mut self, handle: ResizeHandle, delta: i16) -> bool {
        if delta == 0 {
            return false;
        }

        let Content::Container(container) = &self.nodes[handle.container_id].content else {
            return false;
        };
        if handle.divider_index + 1 >= container.children.len() {
            return false;
        }

        let layout = container.layout;
        let left_id = container.children[handle.divider_index];
        let right_id = container.children[handle.divider_index + 1];
        let left_area = self.node_area(left_id);
        let right_area = self.node_area(right_id);

        let (left_size, right_size, gap) = match layout {
            Layout::Vertical => {
                let gap = right_area.x.saturating_sub(left_area.right());
                (left_area.width, right_area.width, gap)
            }
            Layout::Horizontal => {
                let gap = right_area.y.saturating_sub(left_area.bottom());
                (left_area.height, right_area.height, gap)
            }
        };

        let total = left_size.saturating_add(gap).saturating_add(right_size);
        if total <= MIN_LEAF_SIZE.saturating_mul(2) {
            return false;
        }

        let max_left = total.saturating_sub(MIN_LEAF_SIZE).saturating_sub(gap);
        let new_left = (left_size as i32 + delta as i32)
            .clamp(MIN_LEAF_SIZE as i32, max_left as i32) as u16;
        let new_right = total.saturating_sub(gap).saturating_sub(new_left);
        if new_right < MIN_LEAF_SIZE {
            return false;
        }

        let pair_space = new_left.saturating_add(new_right);
        if pair_space == 0 {
            return false;
        }
        let left_fraction = new_left as f32 / pair_space as f32;

        let container = self.container_mut(handle.container_id);
        container.ensure_weights();
        let pair_total = container.weights[handle.divider_index]
            + container.weights[handle.divider_index + 1];
        container.weights[handle.divider_index] = pair_total * left_fraction;
        container.weights[handle.divider_index + 1] = pair_total * (1.0 - left_fraction);
        self.recalculate();
        true
    }

    /// Returns the screen area of a split divider for visual highlighting.
    pub fn resize_divider_area(&self, handle: ResizeHandle) -> Option<Rect> {
        let Content::Container(container) = &self.nodes[handle.container_id].content else {
            return None;
        };
        if handle.divider_index + 1 >= container.children.len() {
            return None;
        }
        let left = self.node_area(container.children[handle.divider_index]);
        let right = self.node_area(container.children[handle.divider_index + 1]);
        match container.layout {
            Layout::Vertical => {
                let x = left.right().saturating_sub(1);
                let width = right.x.saturating_add(1).saturating_sub(x).max(1);
                Some(Rect::new(x, left.y, width, left.height))
            }
            Layout::Horizontal => {
                let y = left.bottom().saturating_sub(1);
                let height = right.y.saturating_add(1).saturating_sub(y).max(1);
                Some(Rect::new(left.x, y, left.width, height))
            }
        }
    }
}

#[derive(Debug)]
pub struct Traverse<'a> {
    tree: &'a Tree,
    stack: Vec<ViewId>, // TODO: reuse the one we use on update
}

impl<'a> Traverse<'a> {
    fn new(tree: &'a Tree) -> Self {
        Self {
            tree,
            stack: vec![tree.root],
        }
    }
}

impl<'a> Iterator for Traverse<'a> {
    type Item = (ViewId, &'a View);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let key = self.stack.pop()?;

            let node = &self.tree.nodes[key];

            match &node.content {
                Content::View(view) => return Some((key, view)),
                Content::AgentPanel(_) | Content::GitPanel(_) | Content::TerminalPanel(_) => continue,
                Content::Container(container) => {
                    self.stack.extend(container.children.iter().rev());
                }
            }
        }
    }
}

impl DoubleEndedIterator for Traverse<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        loop {
            let key = self.stack.pop()?;

            let node = &self.tree.nodes[key];

            match &node.content {
                Content::View(view) => return Some((key, view)),
                Content::AgentPanel(_) | Content::GitPanel(_) | Content::TerminalPanel(_) => continue,
                Content::Container(container) => {
                    self.stack.extend(container.children.iter());
                }
            }
        }
    }
}

pub struct LeafTraverse<'a> {
    tree: &'a Tree,
    stack: Vec<ViewId>,
}

impl<'a> LeafTraverse<'a> {
    fn new(tree: &'a Tree) -> Self {
        Self {
            tree,
            stack: vec![tree.root],
        }
    }
}

impl<'a> Iterator for LeafTraverse<'a> {
    type Item = (ViewId, ());

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let key = self.stack.pop()?;
            let node = &self.tree.nodes[key];
            match &node.content {
                Content::View(_) | Content::AgentPanel(_) | Content::GitPanel(_) | Content::TerminalPanel(_) => {
                    return Some((key, ()))
                }
                Content::Container(container) => {
                    self.stack.extend(container.children.iter().rev());
                }
            }
        }
    }
}

impl DoubleEndedIterator for LeafTraverse<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        loop {
            let key = self.stack.pop()?;
            let node = &self.tree.nodes[key];
            match &node.content {
                Content::View(_) | Content::AgentPanel(_) | Content::GitPanel(_) | Content::TerminalPanel(_) => {
                    return Some((key, ()))
                }
                Content::Container(container) => {
                    self.stack.extend(container.children.iter());
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::editor::GutterConfig;
    use crate::DocumentId;

    #[test]
    fn find_split_in_direction() {
        let mut tree = Tree::new(Rect {
            x: 0,
            y: 0,
            width: 180,
            height: 80,
        });
        let mut view = View::new(DocumentId::default(), GutterConfig::default());
        view.area = Rect::new(0, 0, 180, 80);
        tree.insert(view);

        let l0 = tree.focus;
        let view = View::new(DocumentId::default(), GutterConfig::default());
        tree.split(view, Layout::Vertical);
        let r0 = tree.focus;

        tree.focus = l0;
        let view = View::new(DocumentId::default(), GutterConfig::default());
        tree.split(view, Layout::Horizontal);
        let l1 = tree.focus;

        tree.focus = l0;
        let view = View::new(DocumentId::default(), GutterConfig::default());
        tree.split(view, Layout::Vertical);

        // Tree in test
        // | L0  | L2 |    |
        // |    L1    | R0 |
        let l2 = tree.focus;
        assert_eq!(Some(l0), tree.find_split_in_direction(l2, Direction::Left));
        assert_eq!(Some(l1), tree.find_split_in_direction(l2, Direction::Down));
        assert_eq!(Some(r0), tree.find_split_in_direction(l2, Direction::Right));
        assert_eq!(None, tree.find_split_in_direction(l2, Direction::Up));

        tree.focus = l1;
        assert_eq!(None, tree.find_split_in_direction(l1, Direction::Left));
        assert_eq!(None, tree.find_split_in_direction(l1, Direction::Down));
        assert_eq!(Some(r0), tree.find_split_in_direction(l1, Direction::Right));
        assert_eq!(Some(l0), tree.find_split_in_direction(l1, Direction::Up));

        tree.focus = l0;
        assert_eq!(None, tree.find_split_in_direction(l0, Direction::Left));
        assert_eq!(Some(l1), tree.find_split_in_direction(l0, Direction::Down));
        assert_eq!(Some(l2), tree.find_split_in_direction(l0, Direction::Right));
        assert_eq!(None, tree.find_split_in_direction(l0, Direction::Up));

        tree.focus = r0;
        assert_eq!(Some(l2), tree.find_split_in_direction(r0, Direction::Left));
        assert_eq!(None, tree.find_split_in_direction(r0, Direction::Down));
        assert_eq!(None, tree.find_split_in_direction(r0, Direction::Right));
        assert_eq!(None, tree.find_split_in_direction(r0, Direction::Up));
    }

    #[test]
    fn swap_split_in_direction() {
        let mut tree = Tree::new(Rect {
            x: 0,
            y: 0,
            width: 180,
            height: 80,
        });

        let doc_l0 = DocumentId::default();
        let mut view = View::new(doc_l0, GutterConfig::default());
        view.area = Rect::new(0, 0, 180, 80);
        tree.insert(view);

        let l0 = tree.focus;

        let doc_r0 = DocumentId::default();
        let view = View::new(doc_r0, GutterConfig::default());
        tree.split(view, Layout::Vertical);
        let r0 = tree.focus;

        tree.focus = l0;

        let doc_l1 = DocumentId::default();
        let view = View::new(doc_l1, GutterConfig::default());
        tree.split(view, Layout::Horizontal);
        let l1 = tree.focus;

        tree.focus = l0;

        let doc_l2 = DocumentId::default();
        let view = View::new(doc_l2, GutterConfig::default());
        tree.split(view, Layout::Vertical);
        let l2 = tree.focus;

        // Views in test
        // | L0  | L2 |    |
        // |    L1    | R0 |

        // Document IDs in test
        // | l0  | l2 |    |
        // |    l1    | r0 |

        fn doc_id(tree: &Tree, view_id: ViewId) -> Option<DocumentId> {
            if let Content::View(view) = &tree.nodes[view_id].content {
                Some(view.doc)
            } else {
                None
            }
        }

        tree.focus = l0;
        // `*` marks the view in focus from view table (here L0)
        // | l0*  | l2 |    |
        // |    l1     | r0 |
        tree.swap_split_in_direction(Direction::Down);
        // | l1   | l2 |    |
        // |    l0*    | r0 |
        assert_eq!(tree.focus, l0);
        assert_eq!(doc_id(&tree, l0), Some(doc_l1));
        assert_eq!(doc_id(&tree, l1), Some(doc_l0));
        assert_eq!(doc_id(&tree, l2), Some(doc_l2));
        assert_eq!(doc_id(&tree, r0), Some(doc_r0));

        tree.swap_split_in_direction(Direction::Right);

        // | l1  | l2 |     |
        // |    r0    | l0* |
        assert_eq!(tree.focus, l0);
        assert_eq!(doc_id(&tree, l0), Some(doc_l1));
        assert_eq!(doc_id(&tree, l1), Some(doc_r0));
        assert_eq!(doc_id(&tree, l2), Some(doc_l2));
        assert_eq!(doc_id(&tree, r0), Some(doc_l0));

        // cannot swap, nothing changes
        tree.swap_split_in_direction(Direction::Up);
        // | l1  | l2 |     |
        // |    r0    | l0* |
        assert_eq!(tree.focus, l0);
        assert_eq!(doc_id(&tree, l0), Some(doc_l1));
        assert_eq!(doc_id(&tree, l1), Some(doc_r0));
        assert_eq!(doc_id(&tree, l2), Some(doc_l2));
        assert_eq!(doc_id(&tree, r0), Some(doc_l0));

        // cannot swap, nothing changes
        tree.swap_split_in_direction(Direction::Down);
        // | l1  | l2 |     |
        // |    r0    | l0* |
        assert_eq!(tree.focus, l0);
        assert_eq!(doc_id(&tree, l0), Some(doc_l1));
        assert_eq!(doc_id(&tree, l1), Some(doc_r0));
        assert_eq!(doc_id(&tree, l2), Some(doc_l2));
        assert_eq!(doc_id(&tree, r0), Some(doc_l0));

        tree.focus = l2;
        // | l1  | l2* |    |
        // |    r0     | l0 |

        tree.swap_split_in_direction(Direction::Down);
        // | l1  | r0  |    |
        // |    l2*    | l0 |
        assert_eq!(tree.focus, l2);
        assert_eq!(doc_id(&tree, l0), Some(doc_l1));
        assert_eq!(doc_id(&tree, l1), Some(doc_l2));
        assert_eq!(doc_id(&tree, l2), Some(doc_r0));
        assert_eq!(doc_id(&tree, r0), Some(doc_l0));

        tree.swap_split_in_direction(Direction::Up);
        // | l2* | r0 |    |
        // |    l1    | l0 |
        assert_eq!(tree.focus, l2);
        assert_eq!(doc_id(&tree, l0), Some(doc_l2));
        assert_eq!(doc_id(&tree, l1), Some(doc_l1));
        assert_eq!(doc_id(&tree, l2), Some(doc_r0));
        assert_eq!(doc_id(&tree, r0), Some(doc_l0));
    }

    #[test]
    fn all_vertical_views_have_same_width() {
        let tree_area_width = 180;
        let mut tree = Tree::new(Rect {
            x: 0,
            y: 0,
            width: tree_area_width,
            height: 80,
        });
        let mut view = View::new(DocumentId::default(), GutterConfig::default());
        view.area = Rect::new(0, 0, 180, 80);
        tree.insert(view);

        let view = View::new(DocumentId::default(), GutterConfig::default());
        tree.split(view, Layout::Vertical);

        let view = View::new(DocumentId::default(), GutterConfig::default());
        tree.split(view, Layout::Horizontal);

        tree.remove(tree.focus);

        let view = View::new(DocumentId::default(), GutterConfig::default());
        tree.split(view, Layout::Vertical);

        // Make sure that we only have one level in the tree.
        assert_eq!(3, tree.views().count());
        assert_eq!(
            vec![
                tree_area_width / 3 - 1, // gap here
                tree_area_width / 3 - 1, // gap here
                tree_area_width / 3
            ],
            tree.views()
                .map(|(view, _)| view.area.width)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn vsplit_gap_rounding() {
        let (tree_area_width, tree_area_height) = (80, 24);
        let mut tree = Tree::new(Rect {
            x: 0,
            y: 0,
            width: tree_area_width,
            height: tree_area_height,
        });
        let mut view = View::new(DocumentId::default(), GutterConfig::default());
        view.area = Rect::new(0, 0, tree_area_width, tree_area_height);
        tree.insert(view);

        for _ in 0..9 {
            let view = View::new(DocumentId::default(), GutterConfig::default());
            tree.split(view, Layout::Vertical);
        }

        assert_eq!(10, tree.views().count());
        assert_eq!(
            std::iter::repeat_n(7, 9)
                .chain(Some(8)) // Rounding in `recalculate`.
                .collect::<Vec<_>>(),
            tree.views()
                .map(|(view, _)| view.area.width)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn split_terminal_panel_assigns_area() {
        let mut tree = Tree::new(Rect {
            x: 0,
            y: 0,
            width: 120,
            height: 40,
        });
        let mut view = View::new(DocumentId::default(), GutterConfig::default());
        view.area = Rect::new(0, 0, 120, 40);
        tree.insert(view);

        let panel_id = tree.split_terminal_panel(Layout::Vertical, "session-1".into());
        assert!(tree.is_terminal_panel(panel_id));
        let panel = tree.terminal_panel(panel_id).unwrap();
        assert_eq!(panel.session_id, "session-1");
        assert!(panel.area.width > 0);
        assert!(panel.area.height > 0);
    }

    #[test]
    fn split_git_panel_spans_full_height_from_nested_focus() {
        let tree_area = Rect {
            x: 0,
            y: 0,
            width: 120,
            height: 40,
        };
        let mut tree = Tree::new(tree_area);
        tree.insert(View::new(DocumentId::default(), GutterConfig::default()));
        tree.split(
            View::new(DocumentId::default(), GutterConfig::default()),
            Layout::Horizontal,
        );
        tree.focus = tree.prev();

        let panel_id = tree.split_git_panel(Layout::Vertical);
        let panel = tree.git_panel(panel_id).unwrap();

        assert_eq!(panel.area.y, tree_area.y);
        assert_eq!(panel.area.height, tree_area.height);
        assert_eq!(panel.area.x, tree_area.x);
    }

    #[test]
    fn focused_kind_and_swap_with_panel() {
        let mut tree = Tree::new(Rect {
            x: 0,
            y: 0,
            width: 120,
            height: 40,
        });
        let view_id = tree.insert(View::new(DocumentId::default(), GutterConfig::default()));
        let panel_id = tree.split_git_panel(Layout::Vertical);
        tree.focus = panel_id;

        assert_eq!(tree.focused_kind(), Some(LeafKind::GitPanel));
        assert!(tree.try_focused_view().is_none());

        tree.focus = view_id;
        assert_eq!(tree.focused_kind(), Some(LeafKind::View));
        assert!(tree.try_focused_view().is_some());

        tree.focus = panel_id;
        assert!(tree.swap_split_in_direction(Direction::Right).is_some());
        assert_eq!(tree.focus, panel_id);
    }

    #[test]
    fn resize_handle_at_interior_divider() {
        let mut tree = Tree::new(Rect {
            x: 0,
            y: 0,
            width: 120,
            height: 40,
        });
        tree.insert(View::new(DocumentId::default(), GutterConfig::default()));
        let left = tree.focus;
        tree.split(
            View::new(DocumentId::default(), GutterConfig::default()),
            Layout::Vertical,
        );
        let left_area = tree.get(left).area;
        let divider = left_area.right().saturating_sub(1);
        let row = left_area.y + left_area.height / 2;

        assert!(tree.resize_handle_at(row, divider).is_some());
        assert!(tree
            .resize_handle_at(row, tree.area().x)
            .is_none());
        assert!(tree
            .resize_handle_at(row, tree.area().right().saturating_sub(1))
            .is_none());
    }

    #[test]
    fn adjust_resize_changes_split_ratio() {
        let mut tree = Tree::new(Rect {
            x: 0,
            y: 0,
            width: 120,
            height: 40,
        });
        tree.insert(View::new(DocumentId::default(), GutterConfig::default()));
        tree.split(
            View::new(DocumentId::default(), GutterConfig::default()),
            Layout::Vertical,
        );

        let widths: Vec<_> = tree.views().map(|(view, _)| view.area.width).collect();
        let (handle, _) = tree
            .resize_handle_at(
                tree.area().y + 1,
                tree.get(tree.prev()).area.right().saturating_sub(1),
            )
            .unwrap();
        assert!(tree.adjust_resize(handle, 10));
        let new_widths: Vec<_> = tree.views().map(|(view, _)| view.area.width).collect();
        assert_ne!(widths, new_widths);
        assert!(new_widths[0] > widths[0]);
        assert!(new_widths[1] < widths[1]);
    }

    #[test]
    fn resize_preserves_ratio_on_window_resize() {
        let mut tree = Tree::new(Rect {
            x: 0,
            y: 0,
            width: 120,
            height: 40,
        });
        tree.insert(View::new(DocumentId::default(), GutterConfig::default()));
        let left = tree.focus;
        tree.split(
            View::new(DocumentId::default(), GutterConfig::default()),
            Layout::Vertical,
        );
        tree.set_leaf_weight_fraction(left, 0.25);

        let ratio_before = tree.get(left).area.width as f32 / tree.area().width as f32;
        tree.resize(Rect {
            x: 0,
            y: 0,
            width: 200,
            height: 40,
        });
        let ratio_after = tree.get(left).area.width as f32 / tree.area().width as f32;
        assert!((ratio_before - ratio_after).abs() < 0.05);
    }

    #[test]
    fn swap_split_preserves_weights() {
        let mut tree = Tree::new(Rect {
            x: 0,
            y: 0,
            width: 120,
            height: 40,
        });
        tree.insert(View::new(DocumentId::default(), GutterConfig::default()));
        let left = tree.focus;
        tree.split(
            View::new(DocumentId::default(), GutterConfig::default()),
            Layout::Vertical,
        );
        let right = tree.focus;
        tree.set_leaf_weight_fraction(left, 0.25);
        let right_width_before = tree.get(right).area.width;

        tree.focus = left;
        tree.swap_split_in_direction(Direction::Right);
        let left_width_after = tree.get(left).area.width;
        assert_eq!(right_width_before, left_width_after);
    }
}
