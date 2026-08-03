//! Intermediate representation of the UI tree produced by components.
//!
//! Components produce this tree and the renderer consumes it.

use ratatui::{buffer::Buffer, layout::Rect, style::Style};

/// Opaque identity of a component instance.
pub type ComponentId = u64;

/// Closure used by [`ViewContent::Raw`].
pub type RawDrawFn = dyn Fn(&mut Buffer, Rect) + Send;

/// Visual payload carried by a [`ViewNode`].
pub enum ViewContent {
    /// Paints `style` across the node's area before its children are rendered.
    Container {
        /// Style painted across the node's area.
        style: Style,
    },

    /// Groups children without painting anything itself.
    Fragment,

    /// Single-line text, truncated to the node's width.
    Text {
        /// Text to render.
        text: String,
        /// Text style.
        style: Style,
    },

    /// Custom draw closure.
    Raw(Box<RawDrawFn>),
}

impl std::fmt::Debug for ViewContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Container { .. } => write!(f, "Container"),
            Self::Fragment => write!(f, "Fragment"),
            Self::Text { text, .. } => f.debug_struct("Text").field("text", text).finish(),
            Self::Raw(_) => write!(f, "Raw(<fn>)"),
        }
    }
}

/// Node in the intermediate UI tree produced by component `render` methods.
#[derive(Debug)]
pub struct ViewNode {
    /// Component identity, when the node belongs to a component.
    pub id: Option<ComponentId>,

    /// Terminal area assigned by the layout engine.
    pub area: Rect,

    /// Visual payload of this node.
    pub content: ViewContent,

    /// Child nodes in render order.
    pub children: Vec<ViewNode>,
}

impl ViewNode {
    /// Create a node with the supplied content.
    pub fn new(area: Rect, content: ViewContent) -> Self {
        Self { id: None, area, content, children: Vec::new() }
    }

    /// Create a container with the given children.
    pub fn container(area: Rect, children: Vec<ViewNode>) -> Self {
        Self {
            id: None,
            area,
            content: ViewContent::Container { style: Style::default() },
            children,
        }
    }

    /// Create a transparent grouping node with the given children.
    pub fn fragment(area: Rect, children: Vec<ViewNode>) -> Self {
        Self { id: None, area, content: ViewContent::Fragment, children }
    }

    /// Create a styled container.
    pub fn styled_container(area: Rect, children: Vec<ViewNode>, style: Style) -> Self {
        Self { id: None, area, content: ViewContent::Container { style }, children }
    }

    /// Create a text node.
    pub fn text(area: Rect, text: impl Into<String>, style: Style) -> Self {
        Self {
            id: None,
            area,
            content: ViewContent::Text { text: text.into(), style },
            children: Vec::new(),
        }
    }

    /// Create a node rendered by a custom closure.
    pub fn raw<F>(area: Rect, f: F) -> Self
    where
        F: Fn(&mut Buffer, Rect) + Send + 'static,
    {
        Self { id: None, area, content: ViewContent::Raw(Box::new(f)), children: Vec::new() }
    }

    /// Set the component id.
    pub fn with_id(mut self, id: ComponentId) -> Self {
        self.id = Some(id);
        self
    }

    /// Replace the children list.
    pub fn with_children(mut self, children: Vec<ViewNode>) -> Self {
        self.children = children;
        self
    }

    /// Set the layout area.
    pub fn with_area(mut self, area: Rect) -> Self {
        self.area = area;
        self
    }
}
