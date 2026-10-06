//! Reactive rendering pipeline for TermOxide.
//!
//! This crate provides the terminal-facing rendering pieces: `ViewNode`
//! trees, the `Renderer`, and event routing. The framework-level main loop
//! lives in the `termoxide` crate.
//!
//! ## Modules
//!
//! - [`view_node`]: [`ViewNode`][view_node::ViewNode], the intermediate UI tree that components produce before
//!   rendering.
//! - [`renderer`]: [`Renderer`][renderer::Renderer] walks the tree, calls ratatui draw routines, and accumulates into a
//!   `Buffer` diffed against the previous frame.
//! - [`event_router`]: [`EventRouter`][event_router::EventRouter] maps `termoxide_event` events to component ids and
//!   applies the global key-to-signal bindings.
//!
//! ## Render pipeline
//!
//! ```text
//! 1. Input event arrives
//! 2. LayoutEngine::compute()  →  assigns Rect to every ViewNode
//! 3. Main loop handles the event
//! 4. App::build_view()  →  returns updated ViewNode tree
//! 5. Renderer::render_frame()
//!       ├── draw_node() recursive walk  →  Buffer (current frame)
//!       └── Terminal::draw()  →  diff against previous Buffer  →  stdout
//! 6. ViewNode::mark_clean()  →  clear render bookkeeping for next frame (not implemented yet)
//! ```
//!
//! ## Quick-start
//!
//! ```rust,no_run
//! use std::io::stdout;
//!
//! use ratatui::{Terminal, backend::CrosstermBackend, layout::Rect, style::Style};
//! use termoxide_rendering::{renderer::Renderer, view_node::ViewNode};
//!
//! let backend = CrosstermBackend::new(stdout());
//! let terminal = Terminal::new(backend).unwrap();
//! let mut renderer = Renderer::new(terminal).unwrap();
//! let viewport = renderer.viewport();
//! let mut root = ViewNode::text(viewport, "Hello, TermOxide!", Style::default());
//! renderer.render_frame(&mut root).unwrap();
//! let _ = Rect::default();
//! ```

pub mod builder;
pub mod event_router;
pub mod renderer;
pub mod view_node;
