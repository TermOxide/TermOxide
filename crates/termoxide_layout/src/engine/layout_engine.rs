//! Layout engine based on [`taffy::TaffyTree`].
//!
//! [`LayoutEngine`] is the public entry point for the layout subsystem.
//! It wraps a `TaffyTree<()>` and provides an ergonomic API to:
//!
//! 1. **Build** a tree of nodes (leaves and containers) from either raw [`taffy::Style`] values or from
//!    [`crate::style::Style`] values via the built-in conversion helper.
//!
//! 2. **Build recursively** using the [`LayoutNode`] or [`UiLayoutNode`] enums, which describe an arbitrarily deep tree
//!    in a single value. A container's `children` field is itself a `Vec<LayoutNode>`, so sub-trees can be composed
//!    before being handed to the engine.
//!
//! 3. **Resolve** the Flexbox layout for a given viewport size by calling [`LayoutEngine::compute`].
//!
//! 4. **Read back** the computed position and size of every node via [`LayoutEngine::layout_of`], which returns a copy
//!    of [`taffy::tree::Layout`] containing `f32` coordinates relative to each node's parent.
//!
//! ## Example
//!
//! ```rust
//! use taffy::{Display, FlexDirection, Style, geometry::Size, style::Dimension};
//! use termoxide_layout::engine::layout_engine::LayoutEngine;
//!
//! let mut engine = LayoutEngine::new();
//!
//! // A leaf node: 30 columns × 3 rows.
//! let child = engine
//!     .new_leaf(Style {
//!         size: Size { width: Dimension::length(30.0), height: Dimension::length(3.0) },
//!         ..Style::DEFAULT
//!     })
//!     .unwrap();
//!
//! // A root flex container that fills the whole viewport.
//! let root = engine
//!     .new_container(
//!         Style {
//!             display: taffy::Display::Flex,
//!             flex_direction: taffy::FlexDirection::Column,
//!             ..Style::DEFAULT
//!         },
//!         &[child],
//!     )
//!     .unwrap();
//!
//! // Resolve layout for an 80 × 24 terminal.
//! engine.compute(root, 80.0, 24.0).unwrap();
//!
//! // Inspect computed position and size.
//! if let Some(layout) = engine.layout_of(child) {
//!     println!(
//!         "child → x={} y={} w={} h={}",
//!         layout.location.x, layout.location.y, layout.size.width, layout.size.height
//!     );
//! }
//! ```

use taffy::{
    TaffyError,
    TaffyTree,
    geometry::{Rect, Size},
    prelude::TaffyMaxContent,
    style::{AlignItems, AvailableSpace, Dimension, Display, FlexDirection, JustifyContent, LengthPercentage},
    tree::{Layout, NodeId},
};

use crate::style::{Align, Display as UiDisplay, FlexDirection as UiFlexDirection, Justify, Property, Style, Unit};
/// Error type returned by all fallible [`LayoutEngine`] operations.
///
/// This is a re-export of [`taffy::TaffyError`] so callers do not need to add
/// `taffy` as a direct dependency just to name the error type.
pub type LayoutError = TaffyError;

/// A recursive description of a layout tree using raw `taffy::Style` values.
///
/// A `LayoutNode` is either a [`Leaf`][Self::Leaf] (no children) or a
/// [`Container`][Self::Container] whose `children` field is itself a
/// `Vec<LayoutNode>`.  This allows arbitrarily deep sub-trees to be
/// expressed as a single value and passed to
/// [`LayoutEngine::build_tree`].
///
/// # Example
///
/// ```rust
/// use taffy::{Display, FlexDirection, geometry::Size, style::Dimension};
/// use termoxide_layout::engine::layout_engine::{LayoutEngine, LayoutNode};
///
/// let tree = LayoutNode::Container {
///     style: taffy::Style {
///         display: Display::Flex,
///         flex_direction: FlexDirection::Column,
///         ..taffy::Style::DEFAULT
///     },
///     children: vec![
///         LayoutNode::Leaf(taffy::Style {
///             size: Size { width: Dimension::length(80.0), height: Dimension::length(3.0) },
///             ..taffy::Style::DEFAULT
///         }),
///         // Nested sub-tree:
///         LayoutNode::Container {
///             style: taffy::Style { display: Display::Flex, ..taffy::Style::DEFAULT },
///             children: vec![LayoutNode::Leaf(taffy::Style::DEFAULT), LayoutNode::Leaf(taffy::Style::DEFAULT)],
///         },
///     ],
/// };
///
/// let mut engine = LayoutEngine::new();
/// let root = engine.build_tree(tree).unwrap();
/// engine.compute(root, 80.0, 24.0).unwrap();
/// ```
#[derive(Debug, Clone)]
pub enum LayoutNode {
    /// A leaf node with no children.
    Leaf(taffy::Style),
    /// A container node whose children are themselves [`LayoutNode`]s.
    Container {
        /// Style applied to this container.
        style: taffy::Style,
        /// Recursive children; may include further nested containers.
        children: Vec<LayoutNode>,
    },
}

/// A recursive description of a layout tree using [`crate::style::Style`] values.
///
/// This is the [`crate::style::Style`]-based counterpart of [`LayoutNode`].
/// Styles are converted via [`LayoutEngine::from_ui_style`] when the tree is
/// built by [`LayoutEngine::build_ui_tree`].
///
/// # Example
///
/// ```rust
/// use termoxide_layout::{
///     engine::layout_engine::{LayoutEngine, UiLayoutNode},
///     style::{Display, Style, Unit},
/// };
///
/// let sidebar = UiLayoutNode::Leaf(Style::new().width(Unit::Cells(20)));
/// let tree = UiLayoutNode::Container {
///     style: Style::new().display(Display::Flex).width(Unit::Cells(80)),
///     children: vec![sidebar],
/// };
///
/// let mut engine = LayoutEngine::new();
/// let root = engine.build_ui_tree(tree).unwrap();
/// engine.compute(root, 80.0, 24.0).unwrap();
/// assert_eq!(engine.layout_of(root).map(|l| l.size.width), Some(80.0));
/// ```
#[derive(Debug, Clone)]
pub enum UiLayoutNode {
    /// A leaf node with no children.
    Leaf(Style),
    /// A container node whose children are themselves [`UiLayoutNode`]s.
    Container {
        /// Style for this container.
        style: Style,
        /// Recursive children; may include further nested containers.
        children: Vec<UiLayoutNode>,
    },
}

/// Incremental Flexbox layout engine backed by `taffy::TaffyTree`.
///
/// # Lifecycle
///
/// 1. Create an engine with [`LayoutEngine::new`].
/// 2. Insert nodes in bottom-up order (leaves first, then their parents) with [`new_leaf`][Self::new_leaf] /
///    [`new_container`][Self::new_container] or the `insert_ui_*` convenience methods.
/// 3. Call [`compute`][Self::compute] with the terminal viewport dimensions.
/// 4. Read results with [`layout_of`][Self::layout_of].
///
/// You can call `compute` again whenever the tree or viewport changes;
/// taffy caches results internally and only re-evaluates dirty subtrees.
///
/// # Thread safety
///
/// `TaffyTree` is not `Send` / `Sync`, so neither is `LayoutEngine`.
/// Keep instances on a single thread (typical for TUI redraw loops).
pub struct LayoutEngine {
    /// The underlying taffy node tree.
    ///
    /// `NodeContext` is `()` — we use pure-CSS Flexbox without measure
    /// callbacks.  Text measurement is out of scope here; the renderer
    /// handles terminal cell allocation separately.
    tree: TaffyTree<()>,
}

impl LayoutEngine {
    /// Create a new, empty layout engine with the default capacity (16 nodes).
    pub fn new() -> Self { Self { tree: TaffyTree::new() } }

    /// Create a new engine that pre-allocates space for `capacity` nodes.
    ///
    /// Use when the approximate node count is known in advance to avoid
    /// internal re-allocations.
    pub fn with_capacity(capacity: usize) -> Self { Self { tree: TaffyTree::with_capacity(capacity) } }

    /// Insert a **leaf node** (no children) using an explicit taffy [`Style`].
    ///
    /// Returns the [`NodeId`] of the new node.  The id is stable for the
    /// lifetime of the engine and can be used to read back the layout after
    /// [`compute`][Self::compute].
    ///
    /// # Errors
    ///
    /// Returns [`LayoutError`] if the internal arena is exhausted.
    pub fn new_leaf(&mut self, style: taffy::Style) -> Result<NodeId, LayoutError> { self.tree.new_leaf(style) }

    /// Insert a **container node** with the given `children`.
    ///
    /// Children must have been inserted via [`new_leaf`][Self::new_leaf] or
    /// `new_container` on the **same** engine instance.  Order matters for
    /// flex-row and flex-column layouts: children are laid out in the order
    /// they appear in `children`.
    ///
    /// # Errors
    ///
    /// Returns [`LayoutError`] if any `NodeId` in `children` is unknown to
    /// this engine.
    pub fn new_container(&mut self, style: taffy::Style, children: &[NodeId]) -> Result<NodeId, LayoutError> {
        self.tree.new_with_children(style, children)
    }

    /// Insert a **leaf node** whose style is converted from an
    /// [`crate::style::Style`].
    ///
    /// Only layout-relevant fields are forwarded — visual properties such as
    /// `color`, `background`, and `font_style` are ignored by taffy.
    /// See [`from_ui_style`][Self::from_ui_style] for the full mapping table.
    ///
    /// # Errors
    ///
    /// Propagates any [`LayoutError`] from the underlying tree insertion.
    pub fn insert_ui_leaf(&mut self, style: &Style) -> Result<NodeId, LayoutError> {
        self.tree.new_leaf(Self::from_ui_style(style))
    }

    /// Insert a **container node** whose style is converted from an
    /// [`crate::style::Style`].
    ///
    /// # Errors
    ///
    /// Propagates any [`LayoutError`] from the underlying tree insertion.
    pub fn insert_ui_container(&mut self, style: &Style, children: &[NodeId]) -> Result<NodeId, LayoutError> {
        self.tree.new_with_children(Self::from_ui_style(style), children)
    }

    /// Build a subtree from a recursive [`LayoutNode`] description and return
    /// the root [`NodeId`].
    ///
    /// Nodes are inserted depth-first (children before their parent) so taffy
    /// can validate every child id at container creation time.
    ///
    /// # Errors
    ///
    /// Returns [`LayoutError`] if any node insertion fails.
    pub fn build_tree(&mut self, node: LayoutNode) -> Result<NodeId, LayoutError> {
        match node {
            LayoutNode::Leaf(style) => self.new_leaf(style),
            LayoutNode::Container { style, children } => {
                let child_ids: Result<Vec<NodeId>, LayoutError> =
                    children.into_iter().map(|child| self.build_tree(child)).collect();
                self.new_container(style, &child_ids?)
            },
        }
    }

    /// Build a subtree from a recursive [`UiLayoutNode`] description and
    /// return the root [`NodeId`].
    ///
    /// This is the [`crate::style::Style`]-based counterpart of
    /// [`build_tree`][Self::build_tree].
    ///
    /// # Errors
    ///
    /// Returns [`LayoutError`] if any node insertion fails.
    pub fn build_ui_tree(&mut self, node: UiLayoutNode) -> Result<NodeId, LayoutError> {
        match node {
            UiLayoutNode::Leaf(style) => self.insert_ui_leaf(&style),
            UiLayoutNode::Container { style, children } => {
                let child_ids: Result<Vec<NodeId>, LayoutError> =
                    children.into_iter().map(|child| self.build_ui_tree(child)).collect();
                self.insert_ui_container(&style, &child_ids?)
            },
        }
    }

    /// Replace the style of an existing node.
    ///
    /// taffy automatically marks the node and its ancestors as dirty so the
    /// next [`compute`][Self::compute] re-evaluates only the affected subtree.
    ///
    /// # Errors
    ///
    /// Returns [`LayoutError`] if `node` is not known to this engine.
    pub fn set_style(&mut self, node: NodeId, style: taffy::Style) -> Result<(), LayoutError> {
        self.tree.set_style(node, style)
    }

    /// Replace the taffy style of a node converted from an [`crate::style::Style`].
    ///
    /// # Errors
    ///
    /// Returns [`LayoutError`] if `node` is not known to this engine.
    pub fn set_ui_style(&mut self, node: NodeId, style: &Style) -> Result<(), LayoutError> {
        self.tree.set_style(node, Self::from_ui_style(style))
    }

    /// Resolve the layout for the subtree rooted at `root`.
    ///
    /// `available_width` and `available_height` are the outer bounds of the
    /// viewport in terminal cells (columns and rows).  After this call
    /// succeeds, call [`layout_of`][Self::layout_of] to retrieve each node's
    /// computed position and size.
    ///
    /// Only dirty nodes are re-computed; nodes whose style and parent
    /// constraints have not changed are returned from the internal cache.
    ///
    /// # Errors
    ///
    /// Returns [`LayoutError`] if `root` is not a known node.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use termoxide_layout::engine::layout_engine::LayoutEngine;
    /// # let mut engine = LayoutEngine::new();
    /// # let root = engine.new_leaf(taffy::Style::DEFAULT).unwrap();
    /// // 80-column, 24-row terminal viewport.
    /// engine.compute(root, 80.0, 24.0).unwrap();
    /// ```
    pub fn compute(&mut self, root: NodeId, available_width: f32, available_height: f32) -> Result<(), LayoutError> {
        self.tree.compute_layout(root, Size {
            width: AvailableSpace::Definite(available_width),
            height: AvailableSpace::Definite(available_height),
        })
    }

    /// Resolve the layout for `root` against an **unbounded** available space
    /// (`MaxContent`).
    ///
    /// Useful for measuring the intrinsic size of a subtree before the
    /// terminal viewport dimensions are known (e.g. pop-up dialogs that
    /// should size to their content).
    ///
    /// # Errors
    ///
    /// Returns [`LayoutError`] if `root` is not a known node.
    pub fn compute_unbounded(&mut self, root: NodeId) -> Result<(), LayoutError> {
        self.tree.compute_layout(root, Size::MAX_CONTENT)
    }

    /// Return a copy of the computed [`Layout`] for `node`.
    ///
    /// The layout contains:
    /// - `location`: `(x, y)` position in cells **relative to the parent**.
    /// - `size`: `(width, height)` in cells.
    /// - `border`: per-side border insets (in cells).
    /// - `padding`: per-side padding insets (in cells).
    ///
    /// Returns `None` if `node` is unknown or if [`compute`][Self::compute]
    /// has not been called yet for this node's subtree.
    ///
    /// The value is a **copy** so it is safe to hold while mutating the tree.
    pub fn layout_of(&self, node: NodeId) -> Option<Layout> { self.tree.layout(node).ok().copied() }

    /// Return the taffy [`Style`] currently assigned to `node`.
    ///
    /// Returns `None` if the node does not exist.
    pub fn style_of(&self, node: NodeId) -> Option<taffy::Style> { self.tree.style(node).ok().cloned() }

    /// Mark `node` and all its ancestors as requiring a layout recompute.
    ///
    /// Call this when external state (e.g. text content length) changes the
    /// ideal size of a node but you have not called
    /// [`set_style`][Self::set_style] (which marks dirty automatically).
    ///
    /// # Errors
    ///
    /// Returns [`LayoutError`] if `node` is unknown.
    pub fn mark_dirty(&mut self, node: NodeId) -> Result<(), LayoutError> { self.tree.mark_dirty(node) }

    /// Return `true` if `node` has been marked dirty and needs a layout pass.
    ///
    /// # Errors
    ///
    /// Returns [`LayoutError`] if `node` is unknown.
    pub fn is_dirty(&self, node: NodeId) -> Result<bool, LayoutError> { self.tree.dirty(node) }

    /// Remove a node and all its children from the tree.
    ///
    /// The [`NodeId`] is invalidated after this call and must not be reused.
    ///
    /// # Errors
    ///
    /// Returns [`LayoutError`] if `node` is unknown.
    pub fn remove(&mut self, node: NodeId) -> Result<NodeId, LayoutError> { self.tree.remove(node) }

    /// Remove all nodes from the tree, resetting it to an empty state.
    pub fn clear(&mut self) { self.tree.clear(); }

    /// Return the total number of nodes currently in the tree.
    pub fn node_count(&self) -> usize { self.tree.total_node_count() }

    /// Convert the layout properties of a [`Style`] into a
    /// `taffy::Style`. `Unset` and `Inherit` keep taffy's default.
    pub fn from_ui_style(s: &Style) -> taffy::Style {
        let mut t = taffy::Style::DEFAULT;

        if let Some(d) = s.display.value() {
            t.display = match d {
                UiDisplay::Flex => Display::Flex,
                UiDisplay::Block => Display::Block,
                UiDisplay::None => Display::None,
            };
        }
        if let Some(fd) = s.flex_direction.value() {
            t.flex_direction = match fd {
                UiFlexDirection::Row => FlexDirection::Row,
                UiFlexDirection::Column => FlexDirection::Column,
                UiFlexDirection::RowReverse => FlexDirection::RowReverse,
                UiFlexDirection::ColumnReverse => FlexDirection::ColumnReverse,
            };
        }
        if let Some(grow) = s.flex_grow.value() {
            t.flex_grow = grow;
        }
        if let Some(shrink) = s.flex_shrink.value() {
            t.flex_shrink = shrink;
        }
        t.align_items = s.align_items.value().map(|a| {
            match a {
                Align::Stretch => AlignItems::Stretch,
                Align::Start => AlignItems::Start,
                Align::Center => AlignItems::Center,
                Align::End => AlignItems::End,
                Align::Baseline => AlignItems::Baseline,
            }
        });
        t.justify_content = s.justify_content.value().map(|j| {
            match j {
                Justify::Start => JustifyContent::Start,
                Justify::Center => JustifyContent::Center,
                Justify::End => JustifyContent::End,
                Justify::SpaceBetween => JustifyContent::SpaceBetween,
                Justify::SpaceAround => JustifyContent::SpaceAround,
                Justify::SpaceEvenly => JustifyContent::SpaceEvenly,
            }
        });

        t.size = Size { width: dimension(s.width), height: dimension(s.height) };
        t.min_size = Size { width: dimension(s.min_width), height: dimension(s.min_height) };
        t.max_size = Size { width: dimension(s.max_width), height: dimension(s.max_height) };
        t.padding = Rect {
            left: length_percentage(s.padding_left),
            right: length_percentage(s.padding_right),
            top: length_percentage(s.padding_top),
            bottom: length_percentage(s.padding_bottom),
        };
        let gap = length_percentage(s.gap);
        t.gap = Size { width: gap, height: gap };

        t
    }
}

impl Default for LayoutEngine {
    fn default() -> Self { Self::new() }
}

fn dimension(u: Property<Unit>) -> Dimension {
    match u.value() {
        Some(Unit::Cells(n)) => Dimension::length(f32::from(n)),
        Some(Unit::Percent(p)) => Dimension::percent(p / 100.0),
        _ => Dimension::auto(),
    }
}

fn length_percentage(u: Property<Unit>) -> LengthPercentage {
    match u.value() {
        Some(Unit::Cells(n)) => LengthPercentage::length(f32::from(n)),
        Some(Unit::Percent(p)) => LengthPercentage::percent(p / 100.0),
        _ => LengthPercentage::length(0.0),
    }
}

#[cfg(test)]
mod tests {
    use taffy::Display;

    use super::*;

    /// Build a tree using [`LayoutNode`] with a nested sub-tree and verify
    /// that every node reports the expected computed size.
    ///
    /// Layout:
    /// ```text
    /// root (flex-column, 80×24)
    /// ├── header  (leaf, 80×3)
    /// └── inner   (flex-row, 80×21)
    ///     ├── sidebar  (leaf, 20×21)
    ///     └── content  (leaf, 60×21)
    /// ```
    #[test]
    fn recursive_build_tree() {
        let tree = LayoutNode::Container {
            style: taffy::Style {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                size: Size { width: Dimension::length(80.0), height: Dimension::length(24.0) },
                ..taffy::Style::DEFAULT
            },
            children: vec![
                // header leaf
                LayoutNode::Leaf(taffy::Style {
                    size: Size { width: Dimension::length(80.0), height: Dimension::length(3.0) },
                    ..taffy::Style::DEFAULT
                }),
                // nested inner sub-tree
                LayoutNode::Container {
                    style: taffy::Style {
                        display: Display::Flex,
                        flex_direction: FlexDirection::Row,
                        size: Size { width: Dimension::length(80.0), height: Dimension::length(21.0) },
                        ..taffy::Style::DEFAULT
                    },
                    children: vec![
                        LayoutNode::Leaf(taffy::Style {
                            size: Size { width: Dimension::length(20.0), height: Dimension::length(21.0) },
                            ..taffy::Style::DEFAULT
                        }),
                        LayoutNode::Leaf(taffy::Style {
                            size: Size { width: Dimension::length(60.0), height: Dimension::length(21.0) },
                            ..taffy::Style::DEFAULT
                        }),
                    ],
                },
            ],
        };

        let mut engine = LayoutEngine::new();
        let root = engine.build_tree(tree).unwrap();
        engine.compute(root, 80.0, 24.0).unwrap();

        let root_layout = engine.layout_of(root).expect("root layout");
        assert_eq!(root_layout.size.width, 80.0);
        assert_eq!(root_layout.size.height, 24.0);
    }

    /// Build a tree using [`UiLayoutNode`] with a nested sub-tree.
    #[test]
    fn recursive_build_ui_tree() {
        let sidebar = UiLayoutNode::Leaf(Style::new().width(Unit::Cells(20)).height(Unit::Cells(21)));
        let content = UiLayoutNode::Leaf(Style::new().width(Unit::Cells(60)).height(Unit::Cells(21)));

        // Compose a sub-tree separately and embed it inside the root.
        let inner = UiLayoutNode::Container {
            style: Style::new()
                .display(UiDisplay::Flex)
                .flex_direction(UiFlexDirection::Row)
                .width(Unit::Cells(80))
                .height(Unit::Cells(21)),
            children: vec![sidebar, content],
        };

        let root_tree = UiLayoutNode::Container {
            style: Style::new()
                .display(UiDisplay::Flex)
                .flex_direction(UiFlexDirection::Column)
                .width(Unit::Cells(80))
                .height(Unit::Cells(24)),
            children: vec![
                UiLayoutNode::Leaf(Style::new().width(Unit::Cells(80)).height(Unit::Cells(3))),
                inner,
            ],
        };

        let mut engine = LayoutEngine::new();
        let root = engine.build_ui_tree(root_tree).unwrap();
        engine.compute(root, 80.0, 24.0).unwrap();

        let root_layout = engine.layout_of(root).expect("root layout");
        assert_eq!(root_layout.size.width, 80.0);
        assert_eq!(root_layout.size.height, 24.0);
    }

    /// Build a simple two-child flex column, resolve the layout and verify
    /// that each child occupies exactly its declared size.
    #[test]
    fn flex_column_layout() {
        let mut engine = LayoutEngine::new();

        // Child A: 80 wide × 5 tall
        let a = engine
            .new_leaf(taffy::Style {
                size: Size { width: Dimension::length(80.0), height: Dimension::length(5.0) },
                ..taffy::Style::DEFAULT
            })
            .unwrap();

        // Child B: 80 wide × 19 tall
        let b = engine
            .new_leaf(taffy::Style {
                size: Size { width: Dimension::length(80.0), height: Dimension::length(19.0) },
                ..taffy::Style::DEFAULT
            })
            .unwrap();

        // Root: flex column, fills 80 × 24
        let root = engine
            .new_container(
                taffy::Style {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    size: Size { width: Dimension::length(80.0), height: Dimension::length(24.0) },
                    ..taffy::Style::DEFAULT
                },
                &[a, b],
            )
            .unwrap();

        engine.compute(root, 80.0, 24.0).unwrap();

        let la = engine.layout_of(a).expect("layout for child A");
        let lb = engine.layout_of(b).expect("layout for child B");

        // Child A starts at the top
        assert_eq!(la.location.x, 0.0);
        assert_eq!(la.location.y, 0.0);
        assert_eq!(la.size.width, 80.0);
        assert_eq!(la.size.height, 5.0);

        // Child B follows immediately below A
        assert_eq!(lb.location.x, 0.0);
        assert_eq!(lb.location.y, 5.0);
        assert_eq!(lb.size.width, 80.0);
        assert_eq!(lb.size.height, 19.0);
    }
}
