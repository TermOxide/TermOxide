//! Intermediate representation of the UI tree produced by components.
//!
//! [`ViewNode`] is the **output** of the component layer and the **input** of
//! the renderer.  Every component's `render` method returns a `ViewNode`
//! (or contributes children to one), forming a complete tree that mirrors the
//! logical structure of the application's UI.
//!
//! ## Position in the pipeline
//!
//! ```text
//! Component::render()
//!       │
//!       ▼
//!   ViewNode tree  ◄── this module
//!       │
//!       ▼
//!   Renderer  (traverses the tree, fills a ratatui Buffer)
//!       │
//!       ▼
//!   Terminal output
//! ```
//!
//! ## Design principles
//!
//! - **Owned, heap-allocated tree**: unlike ratatui `Widget` which is consumed
//!   on render, `ViewNode` is a plain data structure that can be inspected,
//!   diffed, or logged independently of the render pass.
//!
//! - **Dirty tracking**: the [`ViewNode::dirty`] flag is set by the reactive
//!   layer whenever a signal that the node depends on changes.  The
//!   [`RenderLoop`][crate::render_loop::RenderLoop] only re-renders the
//!   sub-tree containing at least one dirty node, keeping frame work low.
//!
//! - **Spatial metadata**: every node carries the terminal area ([`Rect`])
//!   assigned to it by the layout engine so that the
//!   [`EventRouter`][crate::event_router::EventRouter] can perform O(nodes)
//!   hit-testing without re-running layout.
//!
//! - **Type-erased content**: [`ViewContent::Raw`] lets any component supply
//!   an arbitrary draw closure, which keeps this crate decoupled from the
//!   full set of ratatui built-in widgets.
//!
//! ## Example
//!
//! ```rust
//! use termoxide_rendering::view_node::{ViewNode, ViewContent};
//! use ratatui::layout::Rect;
//! use ratatui::style::{Style, Color};
//!
//! // A simple text label node.
//! let label = ViewNode::text(
//!     Rect::new(0, 0, 20, 1),
//!     "Hello, TermOxide!",
//!     Style::default().fg(Color::Yellow),
//! );
//!
//! // A container wrapping it.
//! let root = ViewNode::container(Rect::new(0, 0, 80, 24), vec![label]);
//! assert_eq!(root.children.len(), 1);
//! ```

use ratatui::{buffer::Buffer, layout::Rect, style::Style};

// ─────────────────────────────────────────────────────────────────────────── //
//  ComponentId
// ─────────────────────────────────────────────────────────────────────────── //

/// Unique identity of a component instance in the application tree.
///
/// The rendering and event-routing layers treat this as an opaque token.
/// The component layer is responsible for assigning stable ids (e.g. via a
/// monotonic counter) so that the [`EventRouter`][crate::event_router::EventRouter]
/// can route keyboard and mouse events back to the right component without
/// knowing anything about its type.
///
/// `0` is reserved and used as the sentinel "no component" value.
pub type ComponentId = u64;

// ─────────────────────────────────────────────────────────────────────────── //
//  ViewContent
// ─────────────────────────────────────────────────────────────────────────── //

/// The visual payload carried by a [`ViewNode`].
///
/// Each variant maps to a distinct rendering strategy inside
/// [`Renderer::draw_node`][crate::renderer::Renderer].
pub enum ViewContent {
    // ── Structural ────────────────────────────────────────────────────────── //
    /// A pure layout container — renders nothing itself but positions its
    /// children inside [`ViewNode::area`].
    ///
    /// Used for `<div>`-like groupings, flex rows, and any node whose only
    /// job is to own a set of children.
    Container,

    // ── Primitive content ─────────────────────────────────────────────────── //
    /// A single line of styled text.
    ///
    /// The renderer writes `text` into the buffer at `area.x, area.y`,
    /// truncating to `area.width` columns.  For multi-line or wrapped text
    /// use multiple `Text` nodes as children of a `Container`.
    Text {
        /// The string to render, already resolved (no reactive binding here).
        text: String,
        /// Ratatui style (foreground, background, modifiers).
        style: Style,
    },

    // ── Escape hatch ──────────────────────────────────────────────────────── //
    /// Arbitrary draw closure.
    ///
    /// Components that need access to the full ratatui `Buffer` API (borders,
    /// charts, gauges, …) can wrap their `Widget::render` call in a `Raw`
    /// closure.  The closure receives the exact area assigned to this node by
    /// the layout engine.
    ///
    /// # Thread safety
    ///
    /// The closure must be `Send` because the render loop may be driven from
    /// a dedicated thread.  In practice this means captures should either be
    /// `Copy` or wrapped in `Arc`.
    ///
    /// # Example
    ///
    /// ```rust
    /// use termoxide_rendering::view_node::{ViewNode, ViewContent};
    /// use ratatui::layout::Rect;
    /// use ratatui::widgets::{Block, Borders, Widget};
    ///
    /// let area = Rect::new(0, 0, 40, 10);
    /// let node = ViewNode::raw(area, |buf, rect| {
    ///     Block::default().borders(Borders::ALL).render(rect, buf);
    /// });
    /// ```
    Raw(Box<dyn Fn(&mut Buffer, Rect) + Send>),
}

impl std::fmt::Debug for ViewContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Container => write!(f, "Container"),
            Self::Text { text, .. } => f.debug_struct("Text").field("text", text).finish(),
            Self::Raw(_) => write!(f, "Raw(<fn>)"),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────── //
//  ViewNode
// ─────────────────────────────────────────────────────────────────────────── //

/// A node in the intermediate UI tree produced by component `render` methods.
///
/// The tree is rebuilt (wholly or partially) on every frame that contains a
/// dirty node, then handed to the [`Renderer`][crate::renderer::Renderer]
/// which walks it top-down, accumulating draw calls into a ratatui `Buffer`.
///
/// ## Lifecycle
///
/// ```text
/// 1. Signal fires  →  component marked dirty
/// 2. RenderLoop wakes
/// 3. Component::render() returns a new ViewNode subtree
/// 4. Renderer traverses the updated tree → Buffer
/// 5. Buffer diff → write escape sequences to stdout
/// ```
///
/// ## Field reference
///
/// | Field         | Set by        | Used by                          |
/// |---------------|---------------|----------------------------------|
/// | `id`          | Component      | EventRouter (focus / hit-test)   |
/// | `area`        | LayoutEngine   | Renderer, EventRouter            |
/// | `content`     | Component      | Renderer                         |
/// | `children`    | Component      | Renderer (recursive traversal)   |
/// | `dirty`       | Reactive layer | RenderLoop (skip-if-clean)       |
#[derive(Debug)]
pub struct ViewNode {
    /// Optional component identity.
    ///
    /// `None` for anonymous structural nodes (e.g. spacers, wrappers).
    /// `Some(id)` for interactive components that handle keyboard or mouse
    /// events.
    pub id: Option<ComponentId>,

    /// Terminal area (column, row, width, height) assigned by the layout
    /// engine.
    ///
    /// Set after the layout pass, before the render pass.  Initially
    /// [`Rect::default()`] (zero-sized at the origin).
    pub area: Rect,

    /// Visual payload of this node.
    pub content: ViewContent,

    /// Child nodes in document order (top-to-bottom, left-to-right for
    /// `FlexDirection::Row`).
    pub children: Vec<ViewNode>,

    /// Whether this node (or any node in its sub-tree) needs to be redrawn.
    ///
    /// The reactive layer sets this to `true` whenever a signal read during
    /// the node's last render changes.  The [`RenderLoop`][crate::render_loop::RenderLoop]
    /// clears it after a successful render pass.
    pub dirty: bool,
}

// ─────────────────────────────────────────────────────────────────────────── //
//  Constructors
// ─────────────────────────────────────────────────────────────────────────── //

impl ViewNode {
    /// Create a bare node with no content and no children.
    ///
    /// Prefer the typed constructors ([`container`][Self::container],
    /// [`text`][Self::text], [`raw`][Self::raw]) for common use-cases.
    pub fn new(area: Rect, content: ViewContent) -> Self {
        Self {
            id: None,
            area,
            content,
            children: Vec::new(),
            dirty: true,
        }
    }

    /// Create a layout-only container with the given children.
    ///
    /// ```rust
    /// use termoxide_rendering::view_node::ViewNode;
    /// use ratatui::layout::Rect;
    ///
    /// let root = ViewNode::container(Rect::new(0, 0, 80, 24), vec![]);
    /// assert!(matches!(
    ///     root.content,
    ///     termoxide_rendering::view_node::ViewContent::Container
    /// ));
    /// ```
    pub fn container(area: Rect, children: Vec<ViewNode>) -> Self {
        Self {
            id: None,
            area,
            content: ViewContent::Container,
            children,
            dirty: true,
        }
    }

    /// Create a single-line text node.
    ///
    /// ```rust
    /// use termoxide_rendering::view_node::ViewNode;
    /// use ratatui::{layout::Rect, style::Style};
    ///
    /// let node = ViewNode::text(Rect::new(0, 0, 10, 1), "hi", Style::default());
    /// ```
    pub fn text(area: Rect, text: impl Into<String>, style: Style) -> Self {
        Self {
            id: None,
            area,
            content: ViewContent::Text {
                text: text.into(),
                style,
            },
            children: Vec::new(),
            dirty: true,
        }
    }

    /// Create a node whose content is rendered by a closure.
    ///
    /// See [`ViewContent::Raw`] for details and a full example.
    pub fn raw<F>(area: Rect, f: F) -> Self
    where
        F: Fn(&mut Buffer, Rect) + Send + 'static,
    {
        Self {
            id: None,
            area,
            content: ViewContent::Raw(Box::new(f)),
            children: Vec::new(),
            dirty: true,
        }
    }

    // ── Builder methods ────────────────────────────────────────────────────── //

    /// Attach a [`ComponentId`] to this node and return `self`.
    ///
    /// Allows fluent construction:
    /// ```rust
    /// use termoxide_rendering::view_node::ViewNode;
    /// use ratatui::layout::Rect;
    ///
    /// let node = ViewNode::container(Rect::default(), vec![]).with_id(42);
    /// assert_eq!(node.id, Some(42));
    /// ```
    pub fn with_id(mut self, id: ComponentId) -> Self {
        self.id = Some(id);
        self
    }

    /// Replace the children list and return `self`.
    pub fn with_children(mut self, children: Vec<ViewNode>) -> Self {
        self.children = children;
        self
    }

    /// Set the layout area and return `self`.
    ///
    /// Called by the [`LayoutEngine`][termoxide_layout::layout_engine::LayoutEngine]
    /// after computing positions.
    pub fn with_area(mut self, area: Rect) -> Self {
        self.area = area;
        self
    }

    // ── Dirty-tracking helpers ─────────────────────────────────────────────── //

    /// Returns `true` if this node or any descendant is dirty.
    ///
    /// The [`RenderLoop`][crate::render_loop::RenderLoop] calls this before
    /// deciding whether to redraw.
    pub fn is_subtree_dirty(&self) -> bool {
        self.dirty || self.children.iter().any(|c| c.is_subtree_dirty())
    }

    /// Recursively clear the dirty flag on this node and all descendants.
    ///
    /// Called by the renderer after a successful render pass so that the next
    /// poll of [`is_subtree_dirty`][Self::is_subtree_dirty] reflects only new
    /// changes.
    pub fn mark_clean(&mut self) {
        self.dirty = false;
        for child in &mut self.children {
            child.mark_clean();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructors_start_dirty() {
        let container = ViewNode::container(Rect::new(0, 0, 10, 10), vec![]);
        let text = ViewNode::text(Rect::new(0, 0, 10, 1), "x", Style::default());
        let raw = ViewNode::raw(Rect::new(0, 0, 1, 1), |_, _| {});

        assert!(container.dirty);
        assert!(text.dirty);
        assert!(raw.dirty);
    }

    #[test]
    fn mark_clean_is_recursive() {
        let child = ViewNode::text(Rect::new(0, 0, 5, 1), "child", Style::default());
        let mut root = ViewNode::container(Rect::new(0, 0, 10, 5), vec![child]);

        root.mark_clean();

        assert!(!root.dirty);
        assert!(!root.children[0].dirty);
        assert!(!root.is_subtree_dirty());
    }

    #[test]
    fn subtree_dirty_detects_dirty_descendant_when_root_is_clean() {
        let mut child = ViewNode::text(Rect::new(0, 0, 5, 1), "child", Style::default());
        child.dirty = true;

        let mut root = ViewNode::container(Rect::new(0, 0, 10, 5), vec![child]);
        root.dirty = false;

        assert!(root.is_subtree_dirty());
    }

    #[test]
    fn with_children_replaces_existing_children() {
        let original = ViewNode::container(Rect::new(0, 0, 10, 5), vec![]);
        let child = ViewNode::text(Rect::new(0, 0, 2, 1), "x", Style::default());

        let replaced = original.with_children(vec![child]);
        assert_eq!(replaced.children.len(), 1);
    }

    #[test]
    fn deep_tree_mark_clean_does_not_panic() {
        let mut root = ViewNode::text(Rect::new(0, 0, 1, 1), "leaf", Style::default());
        for _ in 0..1024 {
            root = ViewNode::container(Rect::new(0, 0, 1, 1), vec![root]);
        }

        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            root.mark_clean();
            root.is_subtree_dirty()
        }));

        assert!(outcome.is_ok(), "deep recursive traversal panicked");
        assert!(!outcome.unwrap());
    }
}
