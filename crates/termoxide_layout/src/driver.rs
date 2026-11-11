//! The two-pass layer driver (ADR D6.3): solve the base layer, place the
//! overlays, solve them, then hand back the paint order.

use std::fmt;

use crate::{
    geometry::CellRect,
    style::{Property, Unit},
    tree::{NodeId, PaintItem, StyleTree, TreeError},
};

/// Lays out one layer root and its subtree, writing rects through
/// [`StyleTree::set_rect`].
pub trait LayerSolver {
    /// # Errors
    ///
    /// [`TreeError::StaleNode`] if the solver reaches a removed node.
    fn solve(&mut self, tree: &mut StyleTree, root: NodeId, bounds: CellRect) -> Result<(), TreeError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriverError {
    Tree(TreeError),
    /// A centred modal needs a `width` and `height` in cells; this one has
    /// another unit or none.
    UnresolvedModalSize(NodeId),
}

impl From<TreeError> for DriverError {
    fn from(error: TreeError) -> Self { DriverError::Tree(error) }
}

impl fmt::Display for DriverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DriverError::Tree(error) => error.fmt(f),
            DriverError::UnresolvedModalSize(id) => {
                write!(f, "modal {id:?} needs a width and height in cells")
            },
        }
    }
}

impl std::error::Error for DriverError {}

/// Runs both passes over `viewport` and returns the paint order.
///
/// # Errors
///
/// Whatever the solver returns, and [`DriverError::UnresolvedModalSize`] for
/// an overlay that cannot be placed.
pub fn drive(
    tree: &mut StyleTree,
    solver: &mut dyn LayerSolver,
    viewport: CellRect,
) -> Result<Vec<PaintItem>, DriverError> {
    let base: Vec<NodeId> = tree.base_roots().collect();
    for root in base {
        solver.solve(tree, root, viewport)?;
    }
    for (root, bounds) in overlay_bounds(tree, viewport)? {
        solver.solve(tree, root, bounds)?;
    }
    Ok(tree.paint_order(viewport))
}

/// The bounds of each overlay root, in paint order. Every v1 overlay is a
/// modal centred in the viewport (ADR D6.3).
///
/// # Errors
///
/// [`DriverError::UnresolvedModalSize`] for a modal without a size in cells.
pub fn overlay_bounds(tree: &StyleTree, viewport: CellRect) -> Result<Vec<(NodeId, CellRect)>, DriverError> {
    tree.portal_roots()
        .into_iter()
        .map(|(root, _)| Ok((root, centred_modal(tree, root, viewport)?)))
        .collect()
}

/// Centres the modal's presented size in `viewport`, the odd cell going right
/// and down. A modal larger than the viewport starts at its edge and is
/// clipped at paint, never shrunk (ADR D5.7).
fn centred_modal(tree: &StyleTree, root: NodeId, viewport: CellRect) -> Result<CellRect, DriverError> {
    let style = &tree.node(root).ok_or(TreeError::StaleNode(root))?.presented_style;
    let cells = |p: Property<Unit>| {
        p.value()
            .and_then(Unit::cells)
            .and_then(|n| u16::try_from(n).ok())
            .ok_or(DriverError::UnresolvedModalSize(root))
    };
    let (width, height) = (cells(style.width)?, cells(style.height)?);
    Ok(CellRect::new(
        viewport.x.saturating_add(viewport.width.saturating_sub(width) / 2),
        viewport.y.saturating_add(viewport.height.saturating_sub(height) / 2),
        width,
        height,
    ))
}
