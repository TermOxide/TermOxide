use super::{NodeId, StyleTree};
use crate::{
    geometry::{CellRect, padding_box},
    style::Display,
};

/// A paint layer. Keys `(Layer, i32)` paint in ascending order; equal keys
/// keep tree order (ADR D6.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Layer {
    Base,
    Overlay,
}

/// One node to paint, in paint order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaintItem {
    pub id: NodeId,
    pub rect: CellRect,
    /// Where the node may draw: its rect within its ancestors' padding boxes,
    /// up to its layer's root rectangle (ADR D5.7).
    pub clip: CellRect,
    pub layer: (Layer, i32),
}

const BASE: (Layer, i32) = (Layer::Base, 0);

impl StyleTree {
    /// The portal roots outside `Display::None` subtrees, in key order with
    /// equal keys in tree order (ADR D6.2). A root set to the base layer is
    /// not a portal.
    pub(crate) fn portal_roots(&self) -> Vec<(NodeId, (Layer, i32))> {
        let mut portals = Vec::new();
        let mut stack: Vec<NodeId> = self.roots.iter().rev().copied().collect();
        while let Some(id) = stack.pop() {
            let Some(node) = self.displayed(id) else { continue };
            if let Some(key) = node.layer.filter(|&key| node.parent.is_some() || key != BASE) {
                portals.push((id, key));
            }
            stack.extend(node.children.iter().rev());
        }
        portals.sort_by_key(|&(_, key)| key);
        portals
    }

    /// The base layer root nodes, in tree order.
    pub(crate) fn base_roots(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.roots
            .iter()
            .copied()
            .filter(|&id| self.nodes.get(id).is_some_and(|n| n.layer.is_none_or(|key| key == BASE)))
    }

    /// Every node to paint: the base layer in pre-order, then each overlay
    /// in key order, each clipped from `viewport` (ADR D6.1, D6.2).
    ///
    /// `Display::None` subtrees and nodes without a rect are skipped.
    pub fn paint_order(&self, viewport: CellRect) -> Vec<PaintItem> {
        let mut items = Vec::new();
        for root in self.base_roots() {
            self.paint_layer(root, BASE, viewport, &mut items);
        }
        for (root, key) in self.portal_roots() {
            self.paint_layer(root, key, viewport, &mut items);
        }
        items
    }

    fn paint_layer(&self, root: NodeId, layer: (Layer, i32), viewport: CellRect, items: &mut Vec<PaintItem>) {
        let mut stack = vec![(root, viewport)];
        while let Some((id, parent_clip)) = stack.pop() {
            let Some(node) = self.displayed(id) else { continue };
            if id != root && node.layer.is_some() {
                continue;
            }
            let Some(rect) = node.rect else { continue };
            let clip = rect.intersection(parent_clip);
            items.push(PaintItem { id, rect, clip, layer });
            let inner = padding_box(&node.presented_style, rect).intersection(clip);
            stack.extend(node.children.iter().rev().map(|&child| (child, inner)));
        }
    }

    fn displayed(&self, id: NodeId) -> Option<&super::Node> {
        self.nodes
            .get(id)
            .filter(|n| n.presented_style.display.value() != Some(Display::None))
    }
}
