//! Builder API for constructing [`ViewNode`] trees.
//!
//! The builder stores `ViewNode` metadata and constructs containers, text, and
//! runtime nodes. `key`, `classes`, and event handlers are
//! currently retained only by the builder and are dropped during `build`.

use std::collections::HashSet;

use ratatui::{layout::Rect, style::Style};

use crate::view_node::{ComponentId, ViewContent, ViewNode};

pub type EventHandler = Box<dyn Fn(&Event) + Send>;

#[derive(Debug)]
pub struct Event;

/// Token: pass to [`el`] to begin building a container node.
pub struct Container;

/// Token: pass to [`fragment`] to build a transparent grouping node.
pub struct Fragment;

/// Fields shared by node builders.
pub struct CommonFields {
    /// Component identity forwarded to `ViewNode`.
    pub id: Option<ComponentId>,
    /// Reconciliation key, currently unused by `ViewNode`.
    pub key: Option<String>,
    /// Stylesheet classes, currently unused by `ViewNode`.
    pub classes: HashSet<String>,
    /// Event handler, currently unused by `ViewNode`.
    pub event_handler: Option<EventHandler>,
    /// Style applied to this node's area.
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
    fn default() -> Self {
        Self::new()
    }
}

/// A value that can be converted into a [`ViewNode`].
pub trait IntoViewNode {
    fn into_view_node(self) -> ViewNode;
}

impl IntoViewNode for ViewNode {
    fn into_view_node(self) -> ViewNode {
        self
    }
}

impl<T: NodeBuilder> IntoViewNode for T {
    fn into_view_node(self) -> ViewNode {
        self.build()
    }
}

/// Common interface for node builders.
pub trait NodeBuilder: Sized {
    fn common(&mut self) -> &mut CommonFields;
    fn build(self) -> ViewNode;

    /// Set the component id.
    fn id(mut self, id: ComponentId) -> Self {
        self.common().id = Some(id);
        self
    }

    /// Store a reconciliation key for future use.
    fn key(mut self, key: impl Into<String>) -> Self {
        self.common().key = Some(key.into());
        self
    }

    /// Store a CSS-like class name for future use.
    fn class(mut self, class: impl Into<String>) -> Self {
        self.common().classes.insert(class.into());
        self
    }

    /// Apply a ratatui [`Style`] to this node.
    fn style(mut self, style: Style) -> Self {
        self.common().style = style;
        self
    }

    /// Store an event handler for future use.
    fn on_event<F: Fn(&Event) + Send + 'static>(mut self, f: F) -> Self {
        self.common().event_handler = Some(Box::new(f));
        self
    }
}

pub struct ContainerBuilder {
    common: CommonFields,
    children: Vec<ViewNode>,
    area: Rect,
}

impl ContainerBuilder {
    fn new() -> Self {
        Self { common: CommonFields::new(), children: Vec::new(), area: Rect::default() }
    }

    /// Set the terminal area for this container.
    pub fn area(mut self, area: Rect) -> Self {
        self.area = area;
        self
    }

    /// Attach child nodes.
    pub fn children(mut self, children: impl IntoIterator<Item = impl IntoViewNode>) -> Self {
        self.children = children.into_iter().map(IntoViewNode::into_view_node).collect();
        self
    }
}

impl NodeBuilder for ContainerBuilder {
    fn common(&mut self) -> &mut CommonFields {
        &mut self.common
    }

    fn build(self) -> ViewNode {
        ViewNode {
            id: self.common.id,
            area: self.area,
            content: ViewContent::Container { style: self.common.style },
            children: self.children,
        }
    }
}

/// Builder for transparent fragment nodes.
pub struct FragmentBuilder {
    common: CommonFields,
    children: Vec<ViewNode>,
    area: Rect,
}

impl FragmentBuilder {
    fn new() -> Self {
        Self { common: CommonFields::new(), children: Vec::new(), area: Rect::default() }
    }

    pub fn area(mut self, area: Rect) -> Self {
        self.area = area;
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = impl IntoViewNode>) -> Self {
        self.children = children.into_iter().map(IntoViewNode::into_view_node).collect();
        self
    }
}

impl NodeBuilder for FragmentBuilder {
    fn common(&mut self) -> &mut CommonFields {
        &mut self.common
    }

    fn build(self) -> ViewNode {
        ViewNode {
            id: self.common.id,
            area: self.area,
            content: ViewContent::Fragment,
            children: self.children,
        }
    }
}

/// Builder for text nodes.
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
    fn common(&mut self) -> &mut CommonFields {
        &mut self.common
    }

    fn build(self) -> ViewNode {
        let mut node = ViewNode::text(self.area, self.content, self.common.style);

        if let Some(id) = self.common.id {
            node = node.with_id(id);
        }

        node
    }
}

pub fn el(_: Container) -> ContainerBuilder {
    ContainerBuilder::new()
}
pub fn fragment(_: Fragment) -> FragmentBuilder {
    FragmentBuilder::new()
}
pub fn text(content: impl Into<String>) -> TextBuilder {
    TextBuilder::new(content)
}

pub fn raw<F>(draw: F) -> RawBuilder<F>
where
    F: Fn(&mut ratatui::buffer::Buffer, Rect) + Send + 'static,
{
    RawBuilder { common: CommonFields::new(), draw, area: Rect::default() }
}

pub struct RawBuilder<F> {
    common: CommonFields,
    draw: F,
    area: Rect,
}

impl<F> RawBuilder<F> {
    pub fn area(mut self, area: Rect) -> Self {
        self.area = area;
        self
    }
}

impl<F> NodeBuilder for RawBuilder<F>
where
    F: Fn(&mut ratatui::buffer::Buffer, Rect) + Send + 'static,
{
    fn common(&mut self) -> &mut CommonFields {
        &mut self.common
    }

    fn build(self) -> ViewNode {
        let mut node = ViewNode::raw(self.area, self.draw);
        node.id = self.common.id;
        node
    }
}
