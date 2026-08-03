//! Common components built on top of TermOxide's rendering primitives.
//!
//! This crate intentionally depends on `termoxide_rendering`; the rendering
//! crate contains only the runtime tree and primitive builders.

extern crate self as termoxide_components;

mod button;

pub use button::{Button, ButtonBuilder, ButtonProps, Msg, button};
pub use termoxide_component_macros::RsxComponent;

use termoxide_rendering::{
    builder::{IntoViewNode, NodeBuilder},
    view_node::{ViewContent, ViewNode},
};

/// Connects a component marker type to its props and builder.
pub trait Component {
    type Props;
    type Builder: NodeBuilder;

    fn render(props: Self::Props) -> Self::Builder;
}

/// Convert a component or primitive into a view node.
pub fn into_view_node(value: impl IntoViewNode) -> ViewNode {
    value.into_view_node()
}

/// Ensure this crate's component output remains a runtime fragment.
pub fn is_component_node(node: &ViewNode) -> bool {
    matches!(node.content, ViewContent::Fragment)
}
