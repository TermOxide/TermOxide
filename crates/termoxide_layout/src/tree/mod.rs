//! Style nodes in a slotmap arena, written through [`NodeId`] (ADR D3.4).
//!
//! The tree takes an already merged style and never merges on anyone's
//! behalf (ADR D3.5). `Style` holds no hover, focus, active or disabled state
//! (ADR D4.4). A node's style is rewritten in place, with no compiled
//! artifact (ADR D10.1).

mod layer;

use std::{any::Any, fmt, rc::Rc};

pub use layer::{Layer, PaintItem};
use slotmap::SlotMap;

use crate::{
    geometry::CellRect,
    style::{Dirty, Inherited, Style, resolve},
};

slotmap::new_key_type! {
    /// A handle to a node of a [`StyleTree`].
    pub struct NodeId;
}

/// One styled node (SPEC §3).
#[derive(Debug)]
#[non_exhaustive]
pub struct Node {
    /// The style as written, already merged by the caller.
    pub own_style: Style,
    /// `own_style` with inheritance resolved (ADR D4.1).
    pub target_style: Style,
    /// What layout and paint read; a copy of `target_style` in v1 (ADR D9.2).
    pub presented_style: Style,
    pub inherited: Rc<Inherited>,
    pub paint_dirty: bool,
    pub layout_dirty: bool,
    /// Descendants must re-derive their records at the next
    /// [`StyleTree::resolve`] (ADR D10.3).
    pub subtree_paint_dirty: bool,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    /// `Some` makes the node a portal root painted in that layer; `None`
    /// paints in the parent's layer (ADR D6.1, D6.2).
    pub layer: Option<(Layer, i32)>,
    /// The border box, written by whatever lays out the tree.
    pub rect: Option<CellRect>,
    /// Dropped with the node (ADR D3.3).
    pub style_effect: Option<Box<dyn Any>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeError {
    /// The id names a node that was removed.
    StaleNode(NodeId),
}

impl fmt::Display for TreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TreeError::StaleNode(id) => write!(f, "style node {id:?} was removed"),
        }
    }
}

impl std::error::Error for TreeError {}

#[derive(Debug, Default)]
pub struct StyleTree {
    nodes: SlotMap<NodeId, Node>,
    roots: Vec<NodeId>,
    pending: Vec<NodeId>,
}

impl StyleTree {
    pub fn new() -> Self { Self::default() }

    pub fn node(&self, id: NodeId) -> Option<&Node> { self.nodes.get(id) }

    fn get(&self, id: NodeId) -> Result<&Node, TreeError> { self.nodes.get(id).ok_or(TreeError::StaleNode(id)) }

    fn get_mut(&mut self, id: NodeId) -> Result<&mut Node, TreeError> {
        self.nodes.get_mut(id).ok_or(TreeError::StaleNode(id))
    }

    fn records(&self, parent: Option<NodeId>) -> Option<(&Style, &Rc<Inherited>)> {
        let parent = self.nodes.get(parent?)?;
        Some((&parent.target_style, &parent.inherited))
    }

    /// Appends a node as the last child of `parent`, or as a root, resolved
    /// and marked paint- and layout-dirty.
    ///
    /// # Errors
    ///
    /// [`TreeError::StaleNode`] if `parent` was removed.
    pub fn insert(&mut self, parent: Option<NodeId>, own_style: Style) -> Result<NodeId, TreeError> {
        if let Some(parent) = parent {
            self.get(parent)?;
        }
        let (target_style, inherited) = resolve(&own_style, self.records(parent));
        let id = self.nodes.insert(Node {
            own_style,
            target_style,
            presented_style: target_style,
            inherited,
            paint_dirty: true,
            layout_dirty: true,
            subtree_paint_dirty: false,
            parent,
            children: Vec::new(),
            layer: None,
            rect: None,
            style_effect: None,
        });
        match parent {
            Some(parent) => self.get_mut(parent)?.children.push(id),
            None => self.roots.push(id),
        }
        Ok(id)
    }

    /// Removes `id` and its subtree, dropping their effects, and marks the
    /// parent layout-dirty.
    ///
    /// # Errors
    ///
    /// [`TreeError::StaleNode`] if `id` was already removed.
    pub fn remove(&mut self, id: NodeId) -> Result<(), TreeError> {
        let parent = self.get(id)?.parent;
        match parent.and_then(|p| self.nodes.get_mut(p)) {
            Some(parent) => {
                parent.children.retain(|&child| child != id);
                parent.layout_dirty = true;
            },
            None => self.roots.retain(|&root| root != id),
        }
        let mut doomed = vec![id];
        while let Some(next) = doomed.pop() {
            if let Some(node) = self.nodes.remove(next) {
                doomed.extend(node.children);
            }
        }
        Ok(())
    }

    /// Replaces the style of `id` and re-resolves that node only; its
    /// descendants follow at the next [`resolve`](Self::resolve) (ADR D4.2).
    ///
    /// # Errors
    ///
    /// [`TreeError::StaleNode`] if `id` was removed.
    pub fn set_own_style(&mut self, id: NodeId, style: Style) -> Result<Dirty, TreeError> {
        let parent = self.get(id)?.parent;
        let (target, inherited) = resolve(&style, self.records(parent));
        let node = self.get_mut(id)?;
        let dirty = node.own_style.dirty(&style);
        node.own_style = style;
        node.paint_dirty |= dirty.paint;
        node.layout_dirty |= dirty.layout;
        if *node.inherited != *inherited {
            node.inherited = inherited;
        }
        if node.target_style != target {
            node.target_style = target;
            if !node.subtree_paint_dirty {
                node.subtree_paint_dirty = true;
                self.pending.push(id);
            }
        }
        Ok(dirty)
    }

    /// Records the border box layout computed for `id`.
    ///
    /// # Errors
    ///
    /// [`TreeError::StaleNode`] if `id` was removed.
    pub fn set_rect(&mut self, id: NodeId, rect: CellRect) -> Result<(), TreeError> {
        self.get_mut(id)?.rect = Some(rect);
        Ok(())
    }

    /// Moves `id` and the descendants that follow it into `layer`, or back
    /// into its parent's layer with `None`. The subtree is not walked; its
    /// layer is derived at paint (ADR D6.1).
    ///
    /// # Errors
    ///
    /// [`TreeError::StaleNode`] if `id` was removed.
    pub fn set_layer(&mut self, id: NodeId, layer: Option<(Layer, i32)>) -> Result<(), TreeError> {
        let node = self.get_mut(id)?;
        node.paint_dirty |= node.layer != layer;
        node.layer = layer;
        Ok(())
    }

    /// Attaches the effect that writes this node's style, dropping the
    /// previous one (ADR D3.3).
    ///
    /// # Errors
    ///
    /// [`TreeError::StaleNode`] if `id` was removed.
    pub fn set_style_effect(&mut self, id: NodeId, effect: Box<dyn Any>) -> Result<(), TreeError> {
        self.get_mut(id)?.style_effect = Some(effect);
        Ok(())
    }

    /// Returns and clears the paint and layout bits of `id`, for the stage
    /// that consumes them.
    ///
    /// # Errors
    ///
    /// [`TreeError::StaleNode`] if `id` was removed.
    pub fn take_dirty(&mut self, id: NodeId) -> Result<Dirty, TreeError> {
        let node = self.get_mut(id)?;
        let dirty = Dirty { paint: node.paint_dirty, layout: node.layout_dirty };
        node.paint_dirty = false;
        node.layout_dirty = false;
        Ok(dirty)
    }

    /// Stages ② and ③: re-derives the records below every node marked
    /// `subtree_paint_dirty`, stopping where nothing changed, then copies
    /// `target_style` into `presented_style`. Untouched subtrees are not
    /// visited.
    ///
    /// Returns how many descendants were re-derived.
    pub fn resolve(&mut self) -> usize {
        let mut pending = std::mem::take(&mut self.pending);
        pending.sort_by_cached_key(|&id| self.depth(id));
        let mut resolved = 0;
        for id in pending {
            let mut stack = vec![id];
            while let Some(id) = stack.pop() {
                let Some(node) = self.nodes.get_mut(id) else { continue };
                if !node.subtree_paint_dirty {
                    continue;
                }
                node.subtree_paint_dirty = false;
                node.presented_style = node.target_style;
                let (parent_target, parent_record) = (node.target_style, Rc::clone(&node.inherited));
                for child in node.children.clone() {
                    let Some(node) = self.nodes.get_mut(child) else { continue };
                    let (target, inherited) = resolve(&node.own_style, Some((&parent_target, &parent_record)));
                    resolved += 1;
                    node.paint_dirty = true;
                    node.layout_dirty |= node.target_style.dirty(&target).layout;
                    if *node.inherited != *inherited {
                        node.inherited = inherited;
                    }
                    if node.target_style != target {
                        node.target_style = target;
                        node.subtree_paint_dirty = true;
                        stack.push(child);
                    }
                }
            }
        }
        resolved
    }

    fn depth(&self, mut id: NodeId) -> usize {
        let mut depth = 0;
        while let Some(parent) = self.nodes.get(id).and_then(|n| n.parent) {
            depth += 1;
            id = parent;
        }
        depth
    }
}
