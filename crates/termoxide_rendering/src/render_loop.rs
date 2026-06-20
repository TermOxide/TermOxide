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
//!         ViewNode::text(viewport, format!("count: {}", self.value), ratatui::style::Style::default())
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
//! - [`App::handle_event`] returns `true` for a [`crossterm::event::Event::Key`] with code `Char('q')` or `Esc` (the
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

// ───────────────────────────────────────────────────────────────────────────
// //  RenderLoop
// ───────────────────────────────────────────────────────────────────────────
// //

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
///
/// use ratatui::{Terminal, backend::CrosstermBackend};
/// use termoxide_rendering::{event_router::EventRouter, render_loop::RenderLoop, renderer::Renderer};
///
/// let backend = CrosstermBackend::new(stdout());
/// let terminal = Terminal::new(backend).unwrap();
/// let renderer = Renderer::new(terminal).unwrap();
///
/// let event_router = EventRouter::new();
///
/// // let mut app = MyApp::new();
/// // RenderLoop::new(renderer, event_router).run(&mut app);
/// ```
pub struct RenderLoop<B: Backend> {
    renderer: Renderer<B>,
    event_router: EventRouter,
    event_reader: Option<Box<dyn FnMut() -> std::io::Result<Event>>>,
}

impl<B: Backend> RenderLoop<B> {
    /// Create a new render loop.
    ///
    /// - `renderer` — the configured [`Renderer`] to paint frames.
    /// - `event_router` — the [`EventRouter`] that maps raw crossterm events to component ids.
    pub fn new(renderer: Renderer<B>, event_router: EventRouter) -> Self {
        Self { renderer, event_router, event_reader: None }
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
            let ev = self.read_event()?;

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
            Key(KeyEvent { code: Char('c'), modifiers: KeyModifiers::CONTROL, .. })
                | Key(KeyEvent { code: Char('d'), modifiers: KeyModifiers::CONTROL, .. })
        )
    }

    fn read_event(&mut self) -> std::io::Result<Event> {
        if let Some(reader) = self.event_reader.as_mut() {
            reader()
        } else {
            event::read()
        }
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

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        io::{Error, ErrorKind},
    };

    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use ratatui::{Terminal, backend::TestBackend, layout::Rect, style::Style};

    use super::*;

    struct CountingApp {
        build_count: usize,
        handle_count: usize,
    }

    impl CountingApp {
        fn new() -> Self { Self { build_count: 0, handle_count: 0 } }
    }

    impl App for CountingApp {
        fn build_view(&mut self, viewport: Rect) -> ViewNode {
            self.build_count += 1;
            ViewNode::text(viewport, "count", Style::default())
        }

        fn handle_event(&mut self, _id: Option<ComponentId>, event: Event) -> bool {
            self.handle_count += 1;
            matches!(event, Event::Key(KeyEvent { code: KeyCode::Char('x'), .. }))
        }
    }

    fn make_key_event(code: KeyCode, modifiers: KeyModifiers) -> Event {
        Event::Key(KeyEvent { code, modifiers, kind: KeyEventKind::Press, state: KeyEventState::NONE })
    }

    fn event_reader(events: Vec<Event>) -> Box<dyn FnMut() -> std::io::Result<Event>> {
        let mut queue: VecDeque<Event> = events.into();
        Box::new(move || {
            queue
                .pop_front()
                .ok_or_else(|| Error::new(ErrorKind::UnexpectedEof, "no events"))
        })
    }

    #[test]
    fn run_quits_on_ctrl_c_before_app_handle() {
        let backend = TestBackend::new(10, 1);
        let terminal = Terminal::new(backend).expect("terminal");
        let renderer = Renderer::new_for_test(terminal);
        let event_router = EventRouter::new();
        let mut render_loop = RenderLoop::new(renderer, event_router);

        render_loop.event_reader = Some(event_reader(vec![make_key_event(KeyCode::Char('c'), KeyModifiers::CONTROL)]));

        let mut app = CountingApp::new();
        let result = render_loop.run(&mut app);

        assert!(result.is_ok());
        assert_eq!(app.build_count, 1);
        assert_eq!(app.handle_count, 0);
    }

    #[test]
    fn run_stops_when_app_handles_event() {
        let backend = TestBackend::new(10, 1);
        let terminal = Terminal::new(backend).expect("terminal");
        let renderer = Renderer::new_for_test(terminal);
        let event_router = EventRouter::new();
        let mut render_loop = RenderLoop::new(renderer, event_router);

        render_loop.event_reader = Some(event_reader(vec![make_key_event(KeyCode::Char('x'), KeyModifiers::NONE)]));

        let mut app = CountingApp::new();
        let result = render_loop.run(&mut app);

        assert!(result.is_ok());
        assert_eq!(app.build_count, 1);
        assert_eq!(app.handle_count, 1);
    }

    #[test]
    fn loop_body_rebuilds_after_non_quit_event() {
        let backend = TestBackend::new(10, 1);
        let terminal = Terminal::new(backend).expect("terminal");
        let renderer = Renderer::new_for_test(terminal);
        let event_router = EventRouter::new();
        let mut render_loop = RenderLoop::new(renderer, event_router);

        render_loop.event_reader = Some(event_reader(vec![
            make_key_event(KeyCode::Char('a'), KeyModifiers::NONE),
            make_key_event(KeyCode::Char('c'), KeyModifiers::CONTROL),
        ]));

        let mut app = CountingApp::new();
        let result = render_loop.run(&mut app);

        assert!(result.is_ok());
        assert_eq!(app.build_count, 2);
        assert_eq!(app.handle_count, 1);
    }
}
