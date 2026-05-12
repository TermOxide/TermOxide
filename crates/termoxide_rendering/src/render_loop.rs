//! Main application loop: waits for events and drives redraws.
//!
//! [`RenderLoop`] is the beating heart of a TermOxide application.  It owns
//! the [`Renderer`][crate::renderer::Renderer] and the
//! [`EventRouter`][crate::event_router::EventRouter], and orchestrates the
//! complete **event → layout → render → diff → output** cycle.
//!
//! ## High-level cycle
//!
//! ```text
//! ┌──────────────────────────────────────────────────────┐
//! │                     RenderLoop::run()                │
//! │                                                      │
//! │  ┌─────────────┐   ┌──────────────────────────┐      │
//! │  │  crossterm  │   │  blocking event loop     │      │
//! │  │  event      │   │  (event::read)           │      │
//! │  └──────┬──────┘   └──────────┬───────────────┘      │
//! │         │                     ▼                      │
//! │         │          ┌───────────────────────┐         │
//! │         │          │  EventRouter::route() │         │
//! │         │          └──────────┬────────────┘         │
//! │         │                     ▼                      │
//! │         │          ┌───────────────────────┐         │
//! │         │          │  app.build_view()     │         │
//! │         │          └──────────┬────────────┘         │
//! │         │                     ▼                      │
//! │         │          ┌───────────────────────┐         │
//! │         │          │  Renderer::render_    │         │
//! │         │          │       frame()         │         │
//! │         │          └───────────────────────┘         │
//! │         │                                            │
//! │         └──────────────(continue loop)────────────── │
//! └──────────────────────────────────────────────────────┘
//! ```
//!
//! ## Application trait
//!
//! Concrete applications implement [`App`], which is the one seam between the
//! framework's generic render machinery and the user's component tree:
//!
//! ```rust
//! use ratatui::layout::Rect;
//! use termoxide_rendering::{render_loop::App, view_node::ViewNode};
//!
//! struct Counter {
//!     value: u32,
//! }
//!
//! impl App for Counter {
//!     fn build_view(&mut self, viewport: Rect) -> ViewNode {
//!         ViewNode::text(
//!             viewport,
//!             format!("count: {}", self.value),
//!             ratatui::style::Style::default(),
//!         )
//!     }
//!
//!     fn handle_event(
//!         &mut self,
//!         _id: Option<termoxide_rendering::view_node::ComponentId>,
//!         _event: crossterm::event::Event,
//!     ) -> bool {
//!         false // not handled → propagate
//!     }
//! }
//! ```
//!
//! ## Shutdown
//!
//! The loop terminates when:
//! - [`App::handle_event`] returns `true` for a
//!   [`crossterm::event::Event::Key`] with code `Char('q')` or `Esc` (the
//!   application controls this), **or**
//! - [`RenderLoop::quit`] sender is used from another thread.

use crossterm::event::{self, Event, KeyModifiers};
use ratatui::{backend::Backend, layout::Rect};

use crate::{
    event_router::EventRouter,
    renderer::{RenderError, Renderer},
    view_node::{ComponentId, ViewNode},
};

// ───────────────────────────────────────────────────────────────────────────
// //  App trait
// ───────────────────────────────────────────────────────────────────────────
// //

/// Interface between the render loop and the application's component tree.
///
/// Implement this trait on your root application state struct.  The render
/// loop calls [`build_view`][App::build_view] whenever the dirty flag is set,
/// and [`handle_event`][App::handle_event] whenever a crossterm event arrives.
///
/// ## Example
///
/// See the [module-level documentation][super::render_loop] for a complete
/// minimal implementation.
pub trait App {
    /// Return the current [`ViewNode`] tree for the given `viewport`.
    ///
    /// This method is called on **every dirty frame**.  Keep it cheap: avoid
    /// heap-allocating large amounts of data.  The reactive layer ensures it is
    /// only called when something actually changed.
    ///
    /// The root node's area should cover the full `viewport` so that the
    /// layout engine fills the terminal.
    fn build_view(&mut self, viewport: Rect) -> ViewNode;

    /// Handle a routed crossterm event.
    ///
    /// `id` is the [`ComponentId`] that the
    /// [`EventRouter`][crate::event_router::EventRouter] determined should
    /// receive this event.  `None` means the event was not claimed by any
    /// specific component (e.g. a global hotkey).
    ///
    /// Return `true` to signal that the application should quit.
    fn handle_event(&mut self, id: Option<ComponentId>, event: Event) -> bool;
}

// ─────────────────────────────────────────────────────────────────────────── //
//  RenderLoop
// ─────────────────────────────────────────────────────────────────────────── //

/// Main application loop.
///
/// `RenderLoop<B>` is generic over the ratatui backend `B` so that tests can
/// substitute a [`TestBackend`][ratatui::backend::TestBackend] without
/// touching a real terminal.
///
/// ## Construction
///
/// ```rust,no_run
/// use std::io::stdout;
    /// - `event_router` — the [`EventRouter`] that maps raw crossterm events
    ///   to component ids.
    pub fn new(renderer: Renderer<B>, event_router: EventRouter) -> Self {
        Self {
            renderer,
            event_router,
        }
    }

    /// Enter the blocking event loop.
    ///
    /// This method returns only when `app.handle_event` returns `true` or when
    /// an I/O error terminates the loop.
    ///
    /// ## Panics
    ///
    /// Does not panic.  All internal errors are converted to
    /// [`RenderLoopError`] and returned.
    ///
    /// ## Terminal restoration
    ///
    /// The method always attempts to restore the terminal (show cursor, disable
    /// raw mode) before returning, even when an error occurred.
    ///
    /// # Errors
    ///
    /// Returns [`RenderLoopError`] on I/O failure or on a crossterm event
    /// reading error.
    pub fn run<A: App>(&mut self, app: &mut A) -> Result<(), RenderLoopError> {
        let loop_result = self.loop_body(app);
        let restore_result = self.renderer.restore();
        match (loop_result, restore_result) {
            (Err(e), _) => Err(e),
            (Ok(()), Err(e)) => Err(e.into()),
            (Ok(()), Ok(())) => Ok(()),
        }
    }

    // ── Inner loop ───────────────────────────────────────────────────────────
    // //

    fn loop_body<A: App>(&mut self, app: &mut A) -> Result<(), RenderLoopError> {
        // Force an initial render.
        let viewport = self.renderer.viewport();
        let mut root = app.build_view(viewport);
        self.renderer.render_frame(&mut root)?;
        // Build initial spatial index so mouse events are routed immediately.
        self.event_router.sync_hit_map(&root);

        loop {
            let ev = event::read()?;

            // Default quit bindings: Ctrl-C and Ctrl-D.
            if Self::is_quit_event(&ev) {
                return Ok(());
            }

            // Route the event to the appropriate component.
            let target_id = self.event_router.route_event(&ev, &root);

            // Let the application handle it.
            if app.handle_event(target_id, ev) {
                return Ok(());
            }

            // Rebuild the view after every handled event. Finer-grained dirty
            // tracking can be reintroduced later.
            let viewport = self.renderer.viewport();
            root = app.build_view(viewport);
            self.event_router.sync_hit_map(&root);
            self.renderer.render_frame(&mut root)?;
        }
    }

    // ── Helpers ──────────────────────────────────────────────────────────────
    // //

    /// Returns `true` for the built-in quit bindings (Ctrl-C, Ctrl-D).
    ///
    /// Applications that want different quit behaviour should intercept the
    /// event in [`App::handle_event`] and return `true` from there.
    fn is_quit_event(ev: &Event) -> bool {
        use crossterm::event::{Event::Key, KeyCode::Char, KeyEvent};
        matches!(
            ev,
            Key(KeyEvent {
                code: Char('c'),
                modifiers: KeyModifiers::CONTROL,
                ..
            }) | Key(KeyEvent {
                code: Char('d'),
                modifiers: KeyModifiers::CONTROL,
                ..
            })
        )
    }
}

// ───────────────────────────────────────────────────────────────────────────
// //  RenderLoopError
// ───────────────────────────────────────────────────────────────────────────
// //

/// Errors that can terminate the render loop.
#[derive(Debug)]
pub enum RenderLoopError {
    /// An I/O error from the renderer (usually a crossterm write failure).
    Render(RenderError),
}

impl std::fmt::Display for RenderLoopError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Render(e) => write!(f, "render error: {e}"),
        }
    }
}

impl std::error::Error for RenderLoopError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Render(e) => Some(e),
        }
    }
}

impl From<RenderError> for RenderLoopError {
    fn from(e: RenderError) -> Self { Self::Render(e) }
}

impl From<std::io::Error> for RenderLoopError {
    fn from(e: std::io::Error) -> Self { Self::Render(RenderError::Io(e)) }
}
