//! Reactive rendering pipeline for TermOxide.
//!
//! This crate is the layer where application events meet the physical terminal.
//! It orchestrates the complete **event → layout → render → diff → terminal
//! output** cycle that drives every frame of a TermOxide application.
//!
//! ## Architecture overview
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │                        termoxide_rendering                          │
//! │                                                                     │
//! │  ┌─────────────┐   builds   ┌─────────────────────────────────┐    │
//! │  │  Component  │──────────► │         ViewNode tree           │    │
//! │  │  (App impl) │            │  (intermediate representation)  │    │
//! │  └─────────────┘            └────────────────┬────────────────┘    │
//! │                                              │                     │
//! │  ┌─────────────┐            traverses        │                     │
//! │  │  RenderLoop │◄───────────────────────────►│                     │
//! │  │             │                         ┌───▼────────┐            │
//! │  │  (main loop)│                         │  Renderer  │            │
//! │  │             │                         │            │            │
//! │  │  └─events   │                         │ Buffer fill│            │
//! │  │             │                         │ + diff     │            │
//! │  └──────┬──────┘                         └───┬────────┘            │
//! │         │                                    │                     │
//! │  ┌──────▼──────┐   routes to                 │ stdout              │
//! │  │ EventRouter │                             ▼                     │
//! │  │             │              ┌───────────────────────────┐        │
//! │  │ focus/      │              │       Terminal (ratatui)  │        │
//! │  │ hit-test    │              └───────────────────────────┘        │
//! │  └─────────────┘                                                   │
//! └─────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Modules
//!
//! - [`view_node`]: [`ViewNode`][view_node::ViewNode], the intermediate UI tree that components produce before
//!   rendering.
//! - [`renderer`]: [`Renderer`][renderer::Renderer] walks the tree, calls ratatui draw routines, and accumulates into a
//!   `Buffer` diffed against the previous frame.
//! - [`render_loop`]: [`RenderLoop`][render_loop::RenderLoop], the main loop; blocks on `termoxide_event` events and
//!   redraws after input.
//! - [`event_router`]: [`EventRouter`][event_router::EventRouter] maps `termoxide_event` events to component ids and
//!   applies the global key-to-signal bindings.
//!
//! ## Render pipeline — data flow
//!
//! The pipeline below describes a single input-driven frame.  Steps 1–2 are
//! the application and layout layers (outside this crate); steps 3–6 are
//! performed inside `termoxide_rendering`.
//!
//! ```text
//! 1. Input event arrives
//! 2. LayoutEngine::compute()  →  assigns Rect to every ViewNode
//! 3. RenderLoop handles the event
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
//! use termoxide_event::{EventStream, event::Event};
//! use termoxide_rendering::{
//!     event_router::EventRouter,
//!     render_loop::{App, RenderLoop},
//!     renderer::Renderer,
//!     view_node::{ComponentId, ViewNode},
//! };
//!
//! struct Hello;
//!
//! impl App for Hello {
//!     fn build_view(&mut self, viewport: Rect) -> ViewNode {
//!         ViewNode::text(viewport, "Hello, TermOxide!", Style::default())
//!     }
//!
//!     fn handle_event(&mut self, _id: Option<ComponentId>, _ev: Event) -> bool { false }
//! }
//!
//! fn main() {
//!     // Created first: the stream owns raw mode for as long as it lives.
//!     let events = EventStream::new();
//!
//!     let backend = CrosstermBackend::new(stdout());
//!     let terminal = Terminal::new(backend).unwrap();
//!     let renderer = Renderer::new(terminal).unwrap();
//!     let event_router = EventRouter::new();
//!
//!     RenderLoop::new(renderer, event_router, events).run(&mut Hello).unwrap();
//! }
//! ```

pub mod event_router;
pub mod render_loop;
pub mod renderer;
pub mod view_node;
