//! Reactive rendering pipeline for TermOxide.
//!
//! This crate is the layer where reactive signals meet the physical terminal.
//! It orchestrates the complete **dirty → layout → render → diff → terminal
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
//! │  │  ├─signals  │                         │ Buffer fill│            │
//! │  │  └─events   │                         │ + diff     │            │
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
//! - [`view_node`]: [`ViewNode`][view_node::ViewNode], the intermediate UI tree
//!   that components produce before rendering.
//! - [`renderer`]: [`Renderer`][renderer::Renderer] walks the tree, calls
//!   ratatui draw routines, and accumulates into a `Buffer` diffed against the
//!   previous frame.
//! - [`render_loop`]: [`RenderLoop`][render_loop::RenderLoop], the main loop;
//!   blocks on crossterm events and reactive dirty notifications, then triggers
//!   redraws.
//! - [`event_router`]: [`EventRouter`][event_router::EventRouter] maps raw
//!   crossterm events to component ids via focus tracking (keyboard) and
//!   spatial hit-testing (mouse).
//!
//! ## Render pipeline — data flow
//!
//! The pipeline below describes a single dirty frame.  Steps 1–2 are the
//! reactive and layout layers (outside this crate); steps 3–6 are performed
//! inside `termoxide_rendering`.
//!
//! ```text
//! 1. Signal changes  →  reactive effect sets dirty flag
//! 2. LayoutEngine::compute()  →  assigns Rect to every ViewNode
//! 3. RenderLoop wakes (dirty channel fires)
//! 4. App::build_view()  →  returns updated ViewNode tree
//! 5. Renderer::render_frame()
//!       ├── draw_node() recursive walk  →  Buffer (current frame)
//!       └── Terminal::draw()  →  diff against previous Buffer  →  stdout
//! 6. ViewNode::mark_clean()  →  clear dirty flags for next frame
//! ```
//!
//! ## Quick-start
//!
//! ```rust,no_run
//! use std::io::stdout;
//!
//! use crossterm::event::Event;
//! use ratatui::{
//!     Terminal,
//!     backend::CrosstermBackend,
//!     layout::Rect,
//!     style::Style,
//! };
//! use termoxide_rendering::{
//!     event_router::EventRouter,
//!     render_loop::{App, RenderLoop, dirty_channel},
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
//!     fn handle_event(
//!         &mut self,
//!         _id: Option<ComponentId>,
//!         _ev: Event,
//!     ) -> bool {
//!         false
//!     }
//! }
//!
//! fn main() {
//!     let backend = CrosstermBackend::new(stdout());
//!     let terminal = Terminal::new(backend).unwrap();
//!     let renderer = Renderer::new(terminal).unwrap();
//!     let (dirty_tx, dirty_rx) = dirty_channel();
//!     let event_router = EventRouter::new();
//!
//!     // Pass dirty_tx to the reactive layer.
//!     // …
//!
//!     RenderLoop::new(renderer, event_router, dirty_rx)
//!         .run(&mut Hello)
//!         .unwrap();
//! }
//! ```

pub mod event_router;
pub mod input;
pub mod render_loop;
pub mod renderer;
pub mod view_node;
