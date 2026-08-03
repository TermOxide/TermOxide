//! Builder API for constructing [`ViewNode`] trees.
//!
//! Designed to match the real `ViewNode` in `termoxide_rendering::view_node`:
//! - `id: Option<ComponentId>` (u64)
//! - `content: ViewContent` where `Container` is a unit variant, `Text` uses `ratatui::Style`
//! - No `key`, `classes`, `event_handler` on `ViewNode` yet — those are builder-only metadata that will be added to
//!   `ViewNode` when the reconciler and event-router land. For now the builder carries them and drops what `ViewNode`
//!   can't hold yet.

use std::collections::HashSet;

use ratatui::{layout::Rect, style::Style};

use crate::view_node::{ComponentId, ViewContent, ViewNode};

// ─────────────────────────────────────────────────────────────────────────── //
//  Shared primitives
// ─────────────────────────────────────────────────────────────────────────── //

pub type EventHandler = Box<dyn Fn(&Event) + Send>;

#[derive(Debug)]
pub struct Event;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Msg {
    Run,
    CustomEvent,
}

/// Token: pass to [`el`] to begin building a container node.
pub struct Container;

// ─────────────────────────────────────────────────────────────────────────── //
//  CommonFields
// ─────────────────────────────────────────────────────────────────────────── //

/// Fields shared by every node builder.
///
/// Exposed via [`NodeBuilder::common`] so that the trait's default setter
/// implementations can mutate it without knowing the concrete builder type.
///
/// Fields that `ViewNode` doesn't carry yet (`key`, `classes`,
/// `event_handler`) are stored here and will be forwarded once the
/// reconciler and event-router layers add them to `ViewNode`.
pub struct CommonFields {
    /// Maps to `ViewNode::id: Option<ComponentId>` (u64).
    pub id: Option<ComponentId>,
    /// Reconciliation key — not on `ViewNode` yet, carried for future use.
    pub key: Option<String>,
    /// Stylesheet classes — not on `ViewNode` yet, carried for future use.
    pub classes: HashSet<String>,
    /// Event handler — not on `ViewNode` yet, carried for future use.
    pub event_handler: Option<EventHandler>,
    /// Ratatui style applied to this node's area.
    pub style: Style,
}

impl CommonFields {
    pub fn new() -> Self {
        Self {
            id: None,
            key: None,
            classes: HashSet::new(),
            event_handler: None,
            style: Style::default(),
        }
    }
}

impl Default for CommonFields {
    fn default() -> Self { Self::new() }
}

// ─────────────────────────────────────────────────────────────────────────── //
//  IntoViewNode
// ─────────────────────────────────────────────────────────────────────────── //

/// Anything that can become a [`ViewNode`].
///
/// Two impls exist:
/// - `ViewNode` itself — identity, node is already built
/// - Any `T: NodeBuilder` — calls `.build()` automatically
///
/// Lets `.children()` accept both in one call without two method names.
pub trait IntoViewNode {
    fn into_view_node(self) -> ViewNode;
}

impl IntoViewNode for ViewNode {
    fn into_view_node(self) -> ViewNode { self }
}

impl<T: NodeBuilder> IntoViewNode for T {
    fn into_view_node(self) -> ViewNode { self.build() }
}

// ─────────────────────────────────────────────────────────────────────────── //
//  NodeBuilder trait
// ─────────────────────────────────────────────────────────────────────────── //

/// Shared builder contract for every node kind.
///
/// ## Required methods (implement these, get everything else free)
///
/// - `common(&mut self) -> &mut CommonFields`
/// - `build(self) -> ViewNode`
///
/// ## What is NOT here
///
/// `.children()` only exists on [`ContainerBuilder`]. The compiler enforces
/// this — `TextBuilder` and `ButtonBuilder` simply have no such method.
pub trait NodeBuilder: Sized {
    fn common(&mut self) -> &mut CommonFields;
    fn build(self) -> ViewNode;

    // ── Default setters — free for every implementor ──────────────────────── //

    /// Set the numeric component id (maps to `ViewNode::id: Option<u64>`).
    fn id(mut self, id: ComponentId) -> Self {
        self.common().id = Some(id);
        self
    }

    /// Set a reconciliation key (stored in the builder, forwarded to
    /// `ViewNode::key` once that field exists).
    fn key(mut self, key: impl Into<String>) -> Self {
        self.common().key = Some(key.into());
        self
    }

    /// Add a CSS-like class name (stored in the builder, forwarded to
    /// `ViewNode::classes` once that field exists).
    fn class(mut self, class: impl Into<String>) -> Self {
        self.common().classes.insert(class.into());
        self
    }

    /// Apply a ratatui [`Style`] to this node.
    fn style(mut self, style: Style) -> Self {
        self.common().style = style;
        self
    }

    /// Register an event handler (stored in the builder, forwarded to
    /// `ViewNode::event_handler` once that field exists).
    fn on_event<F: Fn(&Event) + Send + 'static>(mut self, f: F) -> Self {
        self.common().event_handler = Some(Box::new(f));
        self
    }
}

// ─────────────────────────────────────────────────────────────────────────── //
//  ContainerBuilder
// ─────────────────────────────────────────────────────────────────────────── //

pub struct ContainerBuilder {
    common: CommonFields,
    children: Vec<ViewNode>,
    /// The area rect — extracted from here into `ViewNode::area` on `.build()`.
    area: Rect,
}

impl ContainerBuilder {
    fn new() -> Self { Self { common: CommonFields::new(), children: Vec::new(), area: Rect::default() } }

    /// Set the terminal area for this container.
    pub fn area(mut self, area: Rect) -> Self {
        self.area = area;
        self
    }

    /// Attach child nodes.
    ///
    /// Accepts any iterator of [`IntoViewNode`] — both already-built
    /// [`ViewNode`]s and any [`NodeBuilder`] (calls `.build()` automatically):
    ///
    /// ```rust
    /// use ratatui::layout::Rect;
    /// use termoxide_rendering::builder::{Container, IntoViewNode, el, text};
    ///
    /// use crate::termoxide_rendering::builder::NodeBuilder;
    ///
    /// let rect = Rect::new(0, 0, 80, 24);
    /// let _ = el(Container)
    ///     .area(rect)
    ///     .children([text("pre-built").build().into_view_node(), text("auto-built").into_view_node()]);
    /// ```
    pub fn children(mut self, children: impl IntoIterator<Item = impl IntoViewNode>) -> Self {
        self.children = children.into_iter().map(IntoViewNode::into_view_node).collect();
        self
    }
}

impl NodeBuilder for ContainerBuilder {
    fn common(&mut self) -> &mut CommonFields { &mut self.common }

    fn build(self) -> ViewNode {
        ViewNode {
            id: self.common.id,
            area: self.area,
            content: ViewContent::Container,
            children: self.children,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────── //
//  TextBuilder
// ─────────────────────────────────────────────────────────────────────────── //

/// Builder for text nodes. No `.children()` — compile error if attempted.
pub struct TextBuilder {
    common: CommonFields,
    content: String,
    area: Rect,
}

impl TextBuilder {
    fn new(content: impl Into<String>) -> Self {
        Self { common: CommonFields::new(), content: content.into(), area: Rect::default() }
    }

    /// Set the terminal area for this text node.
    pub fn area(mut self, area: Rect) -> Self {
        self.area = area;
        self
    }
}

impl NodeBuilder for TextBuilder {
    fn common(&mut self) -> &mut CommonFields { &mut self.common }

    fn build(self) -> ViewNode {
        let mut node = ViewNode::text(self.area, self.content, self.common.style);

        if let Some(id) = self.common.id {
            node = node.with_id(id);
        }

        node
    }
}

// ─────────────────────────────────────────────────────────────────────────── //
//  ButtonBuilder
// ─────────────────────────────────────────────────────────────────────────── //

/// Builder for button nodes. No `.children()` — buttons are leaf nodes.
pub struct ButtonBuilder {
    common: CommonFields,
    #[allow(dead_code)]
    label: String,

    #[allow(dead_code)]
    msg: Msg,
    area: Rect,
}

impl ButtonBuilder {
    fn new(label: impl Into<String>, msg: Msg) -> Self {
        Self { common: CommonFields::new(), label: label.into(), msg, area: Rect::default() }
    }

    pub fn area(mut self, area: Rect) -> Self {
        self.area = area;
        self
    }
}

impl NodeBuilder for ButtonBuilder {
    fn common(&mut self) -> &mut CommonFields { &mut self.common }

    fn build(self) -> ViewNode {
        let mut node = ViewNode::container(self.area, Vec::new());

        if let Some(id) = self.common.id {
            node = node.with_id(id);
        }

        node
    }
}

// ─────────────────────────────────────────────────────────────────────────── //
//  Entry points
// ─────────────────────────────────────────────────────────────────────────── //

pub fn el(_: Container) -> ContainerBuilder { ContainerBuilder::new() }
pub fn text(content: impl Into<String>) -> TextBuilder { TextBuilder::new(content) }
pub fn button(label: impl Into<String>, msg: Msg) -> ButtonBuilder { ButtonBuilder::new(label, msg) }
