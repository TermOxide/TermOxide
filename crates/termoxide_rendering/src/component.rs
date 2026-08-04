//! Component contract for the render layer.
//!
//! `Component` is intentionally minimal for Rdmp1: a component only needs to
//! produce a [`ViewNode`][crate::view_node::ViewNode] tree when asked to render.

use crate::view_node::ViewNode;

/// Contract every TermOxide component must satisfy.
///
/// Components are pure renderers: they do not manage lifecycle hooks here.
pub trait Component {
    /// Render the component into a [`ViewNode`] tree.
    fn render(&self) -> ViewNode;
}


