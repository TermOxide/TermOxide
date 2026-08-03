//! Reactive rendering pipeline for TermOxide.
//!
//! Terminal-facing rendering pieces: `ViewNode` trees, the `Renderer`, and
//! event routing. The framework-level main loop lives in `termoxide`.
//!
//! ## Modules
//!
//! - [`view_node`]: The intermediate UI tree.
//! - [`renderer`]: Tree traversal and terminal painting.
//! - [`event_router`]: Key bindings and event routing.
//! - [`builder`]: Builder helpers for constructing trees.


pub mod builder;
pub mod event_router;
pub mod renderer;
pub mod view_node;
