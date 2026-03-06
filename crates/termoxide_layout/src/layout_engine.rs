//! Layout engine based on [`taffy::TaffyTree`].
//!
//! [`LayoutEngine`] is the public entry point for the layout subsystem.
//! It wraps a `TaffyTree<()>` and provides an ergonomic API to:
//!
//! 1. **Build** a tree of nodes (leaves and containers) from either raw
//!    [`taffy::Style`] values or from [`oxidui_style::Style`] values via
//!    the built-in conversion helper.
//!
//! 2. **Build recursively** using the [`LayoutNode`] or [`UiLayoutNode`]
//!    enums, which describe an arbitrarily deep tree in a single value.
//!    A container's `children` field is itself a `Vec<LayoutNode>`, so
//!    sub-trees can be composed before being handed to the engine.
//!
//! 3. **Resolve** the Flexbox layout for a given viewport size by calling
//!    [`LayoutEngine::compute`].
//!
//! 4. **Read back** the computed position and size of every node via
//!    [`LayoutEngine::layout_of`], which returns a copy of
//!    [`taffy::tree::Layout`] containing `f32` coordinates relative to each
//!    node's parent.
//!
//! ## Example
//!
//! ```rust
//! use termoxide_layout::layout_engine::LayoutEngine;
//! use taffy::{Style, Display, FlexDirection, geometry::Size, style::Dimension};
//!
//! let mut engine = LayoutEngine::new();
//!
//! // A leaf node: 30 columns × 3 rows.
//! let child = engine.new_leaf(Style {
//!     size: Size {
//!         width:  Dimension::length(30.0),
//!         height: Dimension::length(3.0),
//!     },
//!     ..Style::DEFAULT
//! }).unwrap();
//!
//! // A root flex container that fills the whole viewport.
//! let root = engine.new_container(Style {
//!     display:        taffy::Display::Flex,
//!     flex_direction: taffy::FlexDirection::Column,
//!     ..Style::DEFAULT
//! }, &[child]).unwrap();
//!
//! // Resolve layout for an 80 × 24 terminal.
//! engine.compute(root, 80.0, 24.0).unwrap();
//!
//! // Inspect computed position and size.
//! if let Some(layout) = engine.layout_of(child) {
//!     println!("child → x={} y={} w={} h={}",
//!         layout.location.x, layout.location.y,
//!         layout.size.width,  layout.size.height);
//! }
//! ```

use crate::stylesheet::StyleSheet;
use crate::{
    Style,
    layout::{Align, Display as UiDisplay, FlexDirection as UiFlexDirection, Justify},
    unit::Unit,
};
use taffy::{
    TaffyError, TaffyTree,
    geometry::{Rect, Size},
    prelude::TaffyMaxContent,
    style::{
        AlignItems, AvailableSpace, Dimension, Display, FlexDirection, JustifyContent,
        LengthPercentage, LengthPercentageAuto,
    },
    tree::{Layout, NodeId},
};
// ─────────────────────────────────────────────────────────────────────────── //
//  Public type aliases
// ─────────────────────────────────────────────────────────────────────────── //

/// Error type returned by all fallible [`LayoutEngine`] operations.
///
/// This is a re-export of [`taffy::TaffyError`] so callers do not need to add
/// `taffy` as a direct dependency just to name the error type.
pub type LayoutError = TaffyError;

// ─────────────────────────────────────────────────────────────────────────── //
//  Recursive node descriptions
// ─────────────────────────────────────────────────────────────────────────── //

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
/// use termoxide_layout::layout_engine::{LayoutEngine, LayoutNode};
/// use taffy::{Display, FlexDirection, geometry::Size, style::Dimension};
///
/// let tree = LayoutNode::Container {
///     style: taffy::Style {
///         display: Display::Flex,
///         flex_direction: FlexDirection::Column,
///         ..taffy::Style::DEFAULT
///     },
///     children: vec![
///         LayoutNode::Leaf(taffy::Style {
///             size: Size {
///                 width:  Dimension::length(80.0),
///                 height: Dimension::length(3.0),
///             },
///             ..taffy::Style::DEFAULT
///         }),
///         // Nested sub-tree:
///         LayoutNode::Container {
///             style: taffy::Style { display: Display::Flex, ..taffy::Style::DEFAULT },
///             children: vec![
///                 LayoutNode::Leaf(taffy::Style::DEFAULT),
///                 LayoutNode::Leaf(taffy::Style::DEFAULT),
///             ],
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

/// How a [`UiLayoutNode`] obtains its [`oxidui_style::Style`].
///
/// Either supply a style value directly ([`Inline`][Self::Inline]) or name a
/// style that is looked up inside an owned [`StyleSheet`]
/// ([`Named`][Self::Named]).
///
/// # Convenience
///
/// `From<Style>` is implemented for `UiStyleSource`, so you can write
/// `my_style.into()` or simply pass a `Style` where a `UiStyleSource` is
/// expected.
///
/// ```rust
/// use termoxide_layout::layout_engine::{UiStyleSource};
/// use termoxide_layout::stylesheet::StyleSheet;
/// use termoxide_layout::{Style, unit::Unit};
///
/// // Inline:
/// let src1 = UiStyleSource::Inline(Style::new().with_width(Unit::cells(40)));
/// // or via From<Style>:
/// let src2: UiStyleSource = Style::new().with_width(Unit::cells(40)).into();
///
/// // Named (looks up "header" in the sheet at build time):
/// let mut sheet = StyleSheet::new();
/// sheet.register("header", Style::new().with_height(Unit::cells(3)));
/// let src3 = UiStyleSource::Named { sheet, name: "header".into() };
/// ```
#[derive(Debug, Clone)]
pub enum UiStyleSource {
    /// Use this style directly.
    Inline(Style),
    /// Look up `name` in `sheet`; falls back to [`Style::default`] when not
    /// found.
    Named {
        /// The stylesheet to query.
        sheet: StyleSheet,
        /// Name of the style entry.
        name: String,
    },
}

impl From<Style> for UiStyleSource {
    fn from(s: Style) -> Self {
        Self::Inline(s)
    }
}

/// Resolve a [`UiStyleSource`] into a concrete [`Style`].
fn resolve_ui_style(source: UiStyleSource) -> Style {
    match source {
        UiStyleSource::Inline(s) => s,
        UiStyleSource::Named { sheet, name } => sheet.get(&name).cloned().unwrap_or_default(),
    }
}

/// A recursive description of a layout tree using [`oxidui_style::Style`] values.
///
/// This is the [`oxidui_style::Style`]-based counterpart of [`LayoutNode`].
/// Styles are converted via [`LayoutEngine::from_ui_style`] when the tree is
/// built by [`LayoutEngine::build_ui_tree`].
///
/// Each variant accepts a [`UiStyleSource`], so the style can be either an
/// inline value or a named lookup from a [`StyleSheet`].
///
/// # Example
///
/// ```rust
/// use termoxide_layout::layout_engine::{LayoutEngine, UiLayoutNode, UiStyleSource};
/// use termoxide_layout::stylesheet::StyleSheet;
/// use termoxide_layout::{Style, layout::Display as UiDisplay, unit::Unit};
///
/// // Build a stylesheet and register a named style.
/// let mut sheet = StyleSheet::new();
/// sheet.register("sidebar", Style::new().with_width(Unit::cells(20)).with_height(Unit::cells(21)));
///
/// let tree = UiLayoutNode::Container {
///     style: Style::new().with_display(UiDisplay::Flex).into(),
///     children: vec![
///         // Inline style:
///         UiLayoutNode::Leaf(Style::new().with_width(Unit::cells(40)).into()),
///         // Named style from a stylesheet:
///         UiLayoutNode::Leaf(UiStyleSource::Named { sheet, name: "sidebar".into() }),
///     ],
/// };
///
/// let mut engine = LayoutEngine::new();
/// let root = engine.build_ui_tree(tree).unwrap();
/// engine.compute(root, 80.0, 24.0).unwrap();
/// ```
#[derive(Debug, Clone)]
pub enum UiLayoutNode {
    /// A leaf node with no children.
    Leaf(UiStyleSource),
    /// A container node whose children are themselves [`UiLayoutNode`]s.
    Container {
        /// Style source for this container.
        style: UiStyleSource,
        /// Recursive children; may include further nested containers.
        children: Vec<UiLayoutNode>,
    },
}

// ─────────────────────────────────────────────────────────────────────────── //
//  LayoutEngine
// ─────────────────────────────────────────────────────────────────────────── //

/// Incremental Flexbox layout engine backed by `taffy::TaffyTree`.
///
/// # Lifecycle
///
/// 1. Create an engine with [`LayoutEngine::new`].
/// 2. Insert nodes in bottom-up order (leaves first, then their parents)
///    with [`new_leaf`][Self::new_leaf] / [`new_container`][Self::new_container]
///    or the `insert_ui_*` convenience methods.
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
    pub fn new() -> Self {
        Self {
            tree: TaffyTree::new(),
        }
    }

    /// Create a new engine that pre-allocates space for `capacity` nodes.
    ///
    /// Use when the approximate node count is known in advance to avoid
    /// internal re-allocations.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            tree: TaffyTree::with_capacity(capacity),
        }
    }

    // ─────────────────────────────────────────────────────── //
    //  Node insertion
    // ─────────────────────────────────────────────────────── //

    /// Insert a **leaf node** (no children) using an explicit taffy [`Style`].
    ///
    /// Returns the [`NodeId`] of the new node.  The id is stable for the
    /// lifetime of the engine and can be used to read back the layout after
    /// [`compute`][Self::compute].
    ///
    /// # Errors
    ///
    /// Returns [`LayoutError`] if the internal arena is exhausted.
    pub fn new_leaf(&mut self, style: taffy::Style) -> Result<NodeId, LayoutError> {
        self.tree.new_leaf(style)
    }

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
    pub fn new_container(
        &mut self,
        style: taffy::Style,
        children: &[NodeId],
    ) -> Result<NodeId, LayoutError> {
        self.tree.new_with_children(style, children)
    }

    /// Insert a **leaf node** whose style is converted from an
    /// [`oxidui_style::Style`].
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
    /// [`oxidui_style::Style`].
    ///
    /// # Errors
    ///
    /// Propagates any [`LayoutError`] from the underlying tree insertion.
    pub fn insert_ui_container(
        &mut self,
        style: &Style,
        children: &[NodeId],
    ) -> Result<NodeId, LayoutError> {
        self.tree
            .new_with_children(Self::from_ui_style(style), children)
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
                let child_ids: Result<Vec<NodeId>, LayoutError> = children
                    .into_iter()
                    .map(|child| self.build_tree(child))
                    .collect();
                self.new_container(style, &child_ids?)
            }
        }
    }

    /// Build a subtree from a recursive [`UiLayoutNode`] description and
    /// return the root [`NodeId`].
    ///
    /// This is the [`oxidui_style::Style`]-based counterpart of
    /// [`build_tree`][Self::build_tree].
    ///
    /// # Errors
    ///
    /// Returns [`LayoutError`] if any node insertion fails.
    pub fn build_ui_tree(&mut self, node: UiLayoutNode) -> Result<NodeId, LayoutError> {
        match node {
            UiLayoutNode::Leaf(source) => {
                let style = resolve_ui_style(source);
                self.insert_ui_leaf(&style)
            }
            UiLayoutNode::Container {
                style: source,
                children,
            } => {
                let style = resolve_ui_style(source);
                let child_ids: Result<Vec<NodeId>, LayoutError> = children
                    .into_iter()
                    .map(|child| self.build_ui_tree(child))
                    .collect();
                self.insert_ui_container(&style, &child_ids?)
            }
        }
    }

    // ─────────────────────────────────────────────────────── //
    //  Style mutation
    // ─────────────────────────────────────────────────────── //

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

    /// Replace the taffy style of a node converted from an [`oxidui_style::Style`].
    ///
    /// # Errors
    ///
    /// Returns [`LayoutError`] if `node` is not known to this engine.
    pub fn set_ui_style(&mut self, node: NodeId, style: &Style) -> Result<(), LayoutError> {
        self.tree.set_style(node, Self::from_ui_style(style))
    }

    // ─────────────────────────────────────────────────────── //
    //  Layout resolution
    // ─────────────────────────────────────────────────────── //

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
    /// # use termoxide_layout::layout_engine::LayoutEngine;
    /// # let mut engine = LayoutEngine::new();
    /// # let root = engine.new_leaf(taffy::Style::DEFAULT).unwrap();
    /// // 80-column, 24-row terminal viewport.
    /// engine.compute(root, 80.0, 24.0).unwrap();
    /// ```
    pub fn compute(
        &mut self,
        root: NodeId,
        available_width: f32,
        available_height: f32,
    ) -> Result<(), LayoutError> {
        self.tree.compute_layout(
            root,
            Size {
                width: AvailableSpace::Definite(available_width),
                height: AvailableSpace::Definite(available_height),
            },
        )
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

    // ─────────────────────────────────────────────────────── //
    //  Layout readback
    // ─────────────────────────────────────────────────────── //

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
    pub fn layout_of(&self, node: NodeId) -> Option<Layout> {
        self.tree.layout(node).ok().copied()
    }

    /// Return the taffy [`Style`] currently assigned to `node`.
    ///
    /// Returns `None` if the node does not exist.
    pub fn style_of(&self, node: NodeId) -> Option<taffy::Style> {
        self.tree.style(node).ok().cloned()
    }

    /// Mark `node` and all its ancestors as requiring a layout recompute.
    ///
    /// Call this when external state (e.g. text content length) changes the
    /// ideal size of a node but you have not called [`set_style`][Self::set_style]
    /// (which marks dirty automatically).
    ///
    /// # Errors
    ///
    /// Returns [`LayoutError`] if `node` is unknown.
    pub fn mark_dirty(&mut self, node: NodeId) -> Result<(), LayoutError> {
        self.tree.mark_dirty(node)
    }

    /// Return `true` if `node` has been marked dirty and needs a layout pass.
    ///
    /// # Errors
    ///
    /// Returns [`LayoutError`] if `node` is unknown.
    pub fn is_dirty(&self, node: NodeId) -> Result<bool, LayoutError> {
        self.tree.dirty(node)
    }

    /// Remove a node and all its children from the tree.
    ///
    /// The [`NodeId`] is invalidated after this call and must not be reused.
    ///
    /// # Errors
    ///
    /// Returns [`LayoutError`] if `node` is unknown.
    pub fn remove(&mut self, node: NodeId) -> Result<NodeId, LayoutError> {
        self.tree.remove(node)
    }

    /// Remove all nodes from the tree, resetting it to an empty state.
    pub fn clear(&mut self) {
        self.tree.clear();
    }

    /// Return the total number of nodes currently in the tree.
    pub fn node_count(&self) -> usize {
        self.tree.total_node_count()
    }

    // ─────────────────────────────────────────────────────── //
    //  Style conversion (oxidui_style → taffy)
    // ─────────────────────────────────────────────────────── //

    /// Convert an [`oxidui_style::Style`] into the equivalent `taffy::Style`.
    ///
    /// Only layout-relevant fields are translated.  Visual properties
    /// (`color`, `background`, `border` appearance, `font_style`, `opacity`,
    /// `text_align`, `overflow`) do not affect taffy's Flexbox pass and are
    /// silently dropped.
    ///
    /// The table below lists every mapped field and the conversion rules for
    /// each [`Unit`] variant:
    ///
    /// | oxidui field       | taffy field           | Unit mapping                                              |
    /// |--------------------|-----------------------|-----------------------------------------------------------|
    /// | `display`          | `display`             | `Block → Block`, `Flex → Flex`, `None → None`             |
    /// | `flex_direction`   | `flex_direction`      | direct enum mapping                                       |
    /// | `flex_grow`        | `flex_grow`           | `Float.0`                                                 |
    /// | `flex_shrink`      | `flex_shrink`         | `Float.0`                                                 |
    /// | `align_items`      | `align_items`         | direct enum mapping (wrapped in `Some`)                   |
    /// | `justify_content`  | `justify_content`     | direct enum mapping (wrapped in `Some`)                   |
    /// | `width`            | `size.width`          | `Cells(n) → length(n)`, `Percent(p) → percent(p/100)`, else `AUTO` |
    /// | `height`           | `size.height`         | same as width                                             |
    /// | `min_width`        | `min_size.width`      | same as width                                             |
    /// | `min_height`       | `min_size.height`     | same as width                                             |
    /// | `max_width`        | `max_size.width`      | same as width                                             |
    /// | `max_height`       | `max_size.height`     | same as width                                             |
    /// | `padding.*`        | `padding.*`           | `Cells(n) → length(n)`, `Percent(p) → percent(p/100)`, else `ZERO` |
    /// | `margin.*`         | `margin.*`            | `Cells(n) → length(n)`, `Percent(p) → percent(p/100)`, else `AUTO` |
    /// | `gap`              | `gap` (both axes)     | same as padding; `Fill`/`Auto`/`Unset → ZERO`             |
    ///
    /// `Unit::Fill(w)` is not directly representable as a taffy `Dimension`; when
    /// used on `width` or `height` it converts to `AUTO`.  If you need fill
    /// semantics, set `flex_grow` instead.
    pub fn from_ui_style(s: &Style) -> taffy::Style {
        let mut t = taffy::Style::DEFAULT;

        // ── display ──────────────────────────────────────────────────────── //
        if let Some(d) = s.display {
            t.display = match d {
                UiDisplay::Block => Display::Block,
                UiDisplay::Flex => Display::Flex,
                UiDisplay::None => Display::None,
            };
        }

        // ── flex_direction ───────────────────────────────────────────────── //
        if let Some(fd) = s.flex_direction {
            t.flex_direction = match fd {
                UiFlexDirection::Row => FlexDirection::Row,
                UiFlexDirection::Column => FlexDirection::Column,
                UiFlexDirection::RowReverse => FlexDirection::RowReverse,
                UiFlexDirection::ColumnReverse => FlexDirection::ColumnReverse,
            };
        }

        // ── flex_grow / flex_shrink ──────────────────────────────────────── //
        if let Some(grow) = s.flex_grow {
            // Float.0 is a pub f32 field
            t.flex_grow = grow.0;
        }
        if let Some(shrink) = s.flex_shrink {
            t.flex_shrink = shrink.0;
        }

        // ── align_items ──────────────────────────────────────────────────── //
        t.align_items = s.align_items.map(|a| match a {
            Align::Start => AlignItems::Start,
            Align::End => AlignItems::End,
            Align::Center => AlignItems::Center,
            Align::Baseline => AlignItems::Baseline,
            Align::Stretch => AlignItems::Stretch,
        });

        // ── justify_content ──────────────────────────────────────────────── //
        t.justify_content = s.justify_content.map(|j| match j {
            Justify::Start => JustifyContent::Start,
            Justify::End => JustifyContent::End,
            Justify::Center => JustifyContent::Center,
            Justify::SpaceBetween => JustifyContent::SpaceBetween,
            Justify::SpaceAround => JustifyContent::SpaceAround,
            Justify::SpaceEvenly => JustifyContent::SpaceEvenly,
        });

        // ── size ─────────────────────────────────────────────────────────── //
        t.size = Size {
            width: unit_to_dimension(s.width.unwrap_or(Unit::Auto)),
            height: unit_to_dimension(s.height.unwrap_or(Unit::Auto)),
        };

        // ── min_size ─────────────────────────────────────────────────────── //
        t.min_size = Size {
            width: unit_to_dimension(s.min_width.unwrap_or(Unit::Auto)),
            height: unit_to_dimension(s.min_height.unwrap_or(Unit::Auto)),
        };

        // ── max_size ─────────────────────────────────────────────────────── //
        t.max_size = Size {
            width: unit_to_dimension(s.max_width.unwrap_or(Unit::Auto)),
            height: unit_to_dimension(s.max_height.unwrap_or(Unit::Auto)),
        };

        // ── padding ──────────────────────────────────────────────────────── //
        if let Some(p) = s.padding {
            t.padding = Rect {
                left: unit_to_length_percentage(p.left),
                right: unit_to_length_percentage(p.right),
                top: unit_to_length_percentage(p.top),
                bottom: unit_to_length_percentage(p.bottom),
            };
        }

        // ── margin ───────────────────────────────────────────────────────── //
        if let Some(m) = s.margin {
            t.margin = Rect {
                left: unit_to_length_percentage_auto(m.left),
                right: unit_to_length_percentage_auto(m.right),
                top: unit_to_length_percentage_auto(m.top),
                bottom: unit_to_length_percentage_auto(m.bottom),
            };
        }

        // ── gap ──────────────────────────────────────────────────────────── //
        // oxidui_style defines a single scalar `gap`; we broadcast it to both
        // the column (width) and row (height) axes.
        if let Some(g) = s.gap {
            let lp = unit_to_length_percentage(g);
            t.gap = Size {
                width: lp,
                height: lp,
            };
        }

        t
    }
}

impl Default for LayoutEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────── //
//  Private unit/dimension conversion helpers
// ─────────────────────────────────────────────────────────────────────────── //

/// Convert a [`Unit`] to a taffy [`Dimension`].
///
/// Used for `size`, `min_size`, and `max_size` fields in `taffy::Style`.
///
/// | Unit              | Dimension                           |
/// |-------------------|-------------------------------------|
/// | `Cells(n)`        | `length(n as f32)`                  |
/// | `Percent(p)`      | `percent(p as f32 / 100.0)`         |
/// | `Fill(_)`         | `auto()` — set `flex_grow` instead  |
/// | `Auto` / `Unset`  | `auto()`                            |
fn unit_to_dimension(u: Unit) -> Dimension {
    match u {
        Unit::Cells(n) => Dimension::length(n as f32),
        Unit::Percent(p) => Dimension::percent(p as f32 / 100.0),
        _ => Dimension::auto(),
    }
}

/// Convert a [`Unit`] to a taffy [`LengthPercentage`].
///
/// Used for `padding` and `gap` — fields that do not support `auto`.
///
/// | Unit              | LengthPercentage                    |
/// |-------------------|-------------------------------------|
/// | `Cells(n)`        | `length(max(n, 0) as f32)`          |
/// | `Percent(p)`      | `percent(p as f32 / 100.0)`         |
/// | `Fill` / `Auto` / `Unset` | `length(0.0)` (zero)        |
fn unit_to_length_percentage(u: Unit) -> LengthPercentage {
    match u {
        Unit::Cells(n) => LengthPercentage::length(n.max(0) as f32),
        Unit::Percent(p) => LengthPercentage::percent(p as f32 / 100.0),
        _ => LengthPercentage::length(0.0),
    }
}

/// Convert a [`Unit`] to a taffy [`LengthPercentageAuto`].
///
/// Used for `margin` — which supports the `auto` keyword for centering.
///
/// | Unit              | LengthPercentageAuto                |
/// |-------------------|-------------------------------------|
/// | `Cells(n)`        | `length(n as f32)`                  |
/// | `Percent(p)`      | `percent(p as f32 / 100.0)`         |
/// | `Auto` / `Fill` / `Unset` | `auto()`                    |
fn unit_to_length_percentage_auto(u: Unit) -> LengthPercentageAuto {
    match u {
        Unit::Cells(n) => LengthPercentageAuto::length(n as f32),
        Unit::Percent(p) => LengthPercentageAuto::percent(p as f32 / 100.0),
        _ => LengthPercentageAuto::auto(),
    }
}

// ─────────────────────────────────────────────────────────────────────────── //
//  Tests
// ─────────────────────────────────────────────────────────────────────────── //

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Style,
        layout::{Display as UiDisplay, FlexDirection as UiFlexDirection},
        unit::Unit,
    };
    use taffy::Display;

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
                size: Size {
                    width: Dimension::length(80.0),
                    height: Dimension::length(24.0),
                },
                ..taffy::Style::DEFAULT
            },
            children: vec![
                // header leaf
                LayoutNode::Leaf(taffy::Style {
                    size: Size {
                        width: Dimension::length(80.0),
                        height: Dimension::length(3.0),
                    },
                    ..taffy::Style::DEFAULT
                }),
                // nested inner sub-tree
                LayoutNode::Container {
                    style: taffy::Style {
                        display: Display::Flex,
                        flex_direction: FlexDirection::Row,
                        size: Size {
                            width: Dimension::length(80.0),
                            height: Dimension::length(21.0),
                        },
                        ..taffy::Style::DEFAULT
                    },
                    children: vec![
                        LayoutNode::Leaf(taffy::Style {
                            size: Size {
                                width: Dimension::length(20.0),
                                height: Dimension::length(21.0),
                            },
                            ..taffy::Style::DEFAULT
                        }),
                        LayoutNode::Leaf(taffy::Style {
                            size: Size {
                                width: Dimension::length(60.0),
                                height: Dimension::length(21.0),
                            },
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

    /// Build a tree using [`UiLayoutNode`] with inline [`UiStyleSource`]s
    /// and a nested sub-tree.
    #[test]
    fn recursive_build_ui_tree() {
        let sidebar = UiLayoutNode::Leaf(
            Style::new()
                .with_width(Unit::cells(20))
                .with_height(Unit::cells(21))
                .into(),
        );
        let content = UiLayoutNode::Leaf(
            Style::new()
                .with_width(Unit::cells(60))
                .with_height(Unit::cells(21))
                .into(),
        );

        // Compose a sub-tree separately and embed it inside the root.
        let inner = UiLayoutNode::Container {
            style: Style::new()
                .with_display(UiDisplay::Flex)
                .with_flex_direction(UiFlexDirection::Row)
                .with_width(Unit::cells(80))
                .with_height(Unit::cells(21))
                .into(),
            children: vec![sidebar, content],
        };

        let root_tree = UiLayoutNode::Container {
            style: Style::new()
                .with_display(UiDisplay::Flex)
                .with_flex_direction(UiFlexDirection::Column)
                .with_width(Unit::cells(80))
                .with_height(Unit::cells(24))
                .into(),
            children: vec![
                UiLayoutNode::Leaf(
                    Style::new()
                        .with_width(Unit::cells(80))
                        .with_height(Unit::cells(3))
                        .into(),
                ),
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

    /// Build a tree where node styles are looked up from a [`StyleSheet`]
    /// via [`UiStyleSource::Named`].
    #[test]
    fn build_ui_tree_with_stylesheet() {
        use crate::stylesheet::StyleSheet;

        let mut sheet = StyleSheet::new();
        sheet.register(
            "header",
            Style::new()
                .with_width(Unit::cells(80))
                .with_height(Unit::cells(3)),
        );
        sheet.register(
            "body",
            Style::new()
                .with_display(UiDisplay::Flex)
                .with_flex_direction(UiFlexDirection::Row)
                .with_width(Unit::cells(80))
                .with_height(Unit::cells(21)),
        );

        let tree = UiLayoutNode::Container {
            style: Style::new()
                .with_display(UiDisplay::Flex)
                .with_flex_direction(UiFlexDirection::Column)
                .with_width(Unit::cells(80))
                .with_height(Unit::cells(24))
                .into(),
            children: vec![
                // Named lookup: style comes from the stylesheet.
                UiLayoutNode::Leaf(UiStyleSource::Named {
                    sheet: sheet.clone(),
                    name: "header".into(),
                }),
                UiLayoutNode::Container {
                    style: UiStyleSource::Named {
                        sheet,
                        name: "body".into(),
                    },
                    children: vec![
                        UiLayoutNode::Leaf(
                            Style::new()
                                .with_width(Unit::cells(20))
                                .with_height(Unit::cells(21))
                                .into(),
                        ),
                        UiLayoutNode::Leaf(
                            Style::new()
                                .with_width(Unit::cells(60))
                                .with_height(Unit::cells(21))
                                .into(),
                        ),
                    ],
                },
            ],
        };

        let mut engine = LayoutEngine::new();
        let root = engine.build_ui_tree(tree).unwrap();
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
                size: Size {
                    width: Dimension::length(80.0),
                    height: Dimension::length(5.0),
                },
                ..taffy::Style::DEFAULT
            })
            .unwrap();

        // Child B: 80 wide × 19 tall
        let b = engine
            .new_leaf(taffy::Style {
                size: Size {
                    width: Dimension::length(80.0),
                    height: Dimension::length(19.0),
                },
                ..taffy::Style::DEFAULT
            })
            .unwrap();

        // Root: flex column, fills 80 × 24
        let root = engine
            .new_container(
                taffy::Style {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    size: Size {
                        width: Dimension::length(80.0),
                        height: Dimension::length(24.0),
                    },
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

    /// Verify that [`from_ui_style`][LayoutEngine::from_ui_style] correctly
    /// maps a basic [`oxidui_style::Style`] into a taffy [`Style`].
    #[test]
    fn from_ui_style_display_flex() {
        let ui = Style::new()
            .with_display(UiDisplay::Flex)
            .with_flex_direction(UiFlexDirection::Row)
            .with_width(Unit::cells(40))
            .with_height(Unit::percent(100))
            .with_padding_all(Unit::cells(1));

        let ts = LayoutEngine::from_ui_style(&ui);

        assert_eq!(ts.display, Display::Flex);
        assert_eq!(ts.flex_direction, FlexDirection::Row);
        assert_eq!(ts.size.width, Dimension::length(40.0));
        assert_eq!(ts.size.height, Dimension::percent(1.0));
        // Padding left should be 1 cell
        assert_eq!(ts.padding.left, LengthPercentage::length(1.0));
    }
}
