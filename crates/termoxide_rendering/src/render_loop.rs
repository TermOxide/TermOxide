//! Main application loop: polls for events and drives redraws.
//!
//! [`RenderLoop`] is the beating heart of a TermOxide application.  It owns
//! the [`Renderer`][crate::renderer::Renderer], the
//! [`EventRouter`][crate::event_router::EventRouter] and an [`EventSource`],
//! and orchestrates the complete **event → layout → render → diff → output**
//! cycle.
//!
//! ## High-level cycle
//!
//! The loop is **frame-paced**, not input-paced: input is drained without
//! blocking, so each iteration sets its own tempo by sleeping out the rest of
//! the frame budget.  A frame only rebuilds and repaints when something
//! actually changed — an event was handled, or the viewport was resized.
//!
//! ```text
//! ┌──────────────────────────────────────────────────────┐
//! │                     RenderLoop::run()                │
//! │                                                      │
//! │  ┌─────────────┐   ┌──────────────────────────┐      │
//! │  │ EventSource │──▶│  drain (non-blocking)    │      │
//! │  └─────────────┘   └──────────┬───────────────┘      │
//! │                               ▼                      │
//! │                    ┌───────────────────────┐         │
//! │                    │  EventRouter::route() │         │
//! │                    └──────────┬────────────┘         │
//! │                               ▼                      │
//! │                    ┌───────────────────────┐         │
//! │                    │  app.build_view()     │  if     │
//! │                    └──────────┬────────────┘  dirty  │
//! │                               ▼                      │
//! │                    ┌───────────────────────┐         │
//! │                    │  Renderer::render_    │         │
//! │                    │       frame()         │         │
//! │                    └──────────┬────────────┘         │
//! │                               ▼                      │
//! │                    ┌───────────────────────┐         │
//! │                    │  sleep(rest of frame) │         │
//! │                    └───────────────────────┘         │
//! │                               │                      │
//! │         ┌─────────────(continue loop)◀───────────────┤
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
//!         _event: termoxide_event::event::Event,
//!     ) -> bool {
//!         false // not handled → propagate
//!     }
//! }
//! ```
//!
//! ## Shutdown
//!
//! The loop terminates when:
//! - the built-in quit bindings fire (Ctrl-C or Ctrl-D), **or**
//! - [`App::handle_event`] returns `true` (the application controls this).

use std::{
    thread,
    time::{Duration, Instant},
};

use ratatui::{backend::Backend, layout::Rect};
use termoxide_event::{
    EventStream,
    event::{Event, KeyCode, KeyModifiers},
};

use crate::{
    event_router::EventRouter,
    renderer::{RenderError, Renderer},
    view_node::{ComponentId, ViewNode},
};

/// Target duration of one loop iteration (~60 frames per second).
///
/// Because [`EventSource::poll_events`] never blocks, this budget is the only
/// thing keeping the loop from spinning on an idle terminal.
pub const FRAME_INTERVAL: Duration = Duration::from_millis(16);

// ───────────────────────────────────────────────────────────────────────────
// //  EventSource
// ───────────────────────────────────────────────────────────────────────────
// //

/// Non-blocking source of input events feeding the render loop.
///
/// The production implementation is [`EventStream`]; tests substitute their own
/// so the loop can be exercised without a real terminal.
pub trait EventSource {
    /// Return every event available right now, oldest first.
    ///
    /// Must **not** block: an empty vector simply means "nothing pending".
    fn poll_events(&mut self) -> Vec<Event>;
}

impl EventSource for EventStream {
    fn poll_events(&mut self) -> Vec<Event> { EventStream::poll_events(self) }
}

// ───────────────────────────────────────────────────────────────────────────
// //  App trait
// ───────────────────────────────────────────────────────────────────────────
// //

/// Interface between the render loop and the application's component tree.
///
/// Implement this trait on your root application state struct.  The render
/// loop calls [`build_view`][App::build_view] whenever the dirty flag is set,
/// and [`handle_event`][App::handle_event] whenever an input event arrives.
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

    /// Handle a routed input event.
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
/// use termoxide_event::EventStream;
/// use termoxide_rendering::{event_router::EventRouter, render_loop::RenderLoop, renderer::Renderer};
///
/// // The event stream owns raw mode, so it must be created *before* the
/// // renderer enters the alternate screen.
/// let events = EventStream::new();
///
/// let backend = CrosstermBackend::new(stdout());
/// let terminal = Terminal::new(backend).unwrap();
/// let renderer = Renderer::new(terminal).unwrap();
///
/// let event_router = EventRouter::new();
///
/// // let mut app = MyApp::new();
/// // RenderLoop::new(renderer, event_router, events).run(&mut app);
/// ```
///
/// ## Terminal restoration order
///
/// The field order below is load-bearing. Rust drops fields in declaration
/// order, so `renderer` is torn down (leaving the alternate screen, restoring
/// the cursor) *before* the event source drops and disables raw mode. Swapping
/// them would cut raw mode while the alternate screen is still up.
pub struct RenderLoop<B: Backend> {
    renderer: Renderer<B>,
    event_router: EventRouter,
    event_source: Box<dyn EventSource>,
}

impl<B: Backend> RenderLoop<B> {
    /// Create a new render loop.
    ///
    /// - `renderer` — the configured [`Renderer`] to paint frames.
    /// - `event_router` — the [`EventRouter`] that maps input events to component ids.
    /// - `event_source` — where input comes from; [`EventStream`] in production.
    pub fn new(renderer: Renderer<B>, event_router: EventRouter, event_source: impl EventSource + 'static) -> Self {
        Self { renderer, event_router, event_source: Box::new(event_source) }
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
        let mut viewport = self.renderer.viewport();
        let mut root = app.build_view(viewport);
        self.renderer.render_frame(&mut root)?;
        // Build initial spatial index so events are routed immediately.
        self.event_router.sync_hit_map(&root);

        loop {
            let frame_start = Instant::now();
            let mut dirty = false;

            for ev in self.event_source.poll_events() {
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

                dirty = true;
            }

            // The input stream carries no resize event, so the viewport is
            // sampled every frame instead. This also catches resizes that
            // happen while the user is not typing at all.
            let current_viewport = self.renderer.viewport();
            if current_viewport != viewport {
                viewport = current_viewport;
                dirty = true;
            }

            // Rebuild the view whenever something changed. Finer-grained dirty
            // tracking can be reintroduced later.
            if dirty {
                root = app.build_view(viewport);
                self.event_router.sync_hit_map(&root);
                self.renderer.render_frame(&mut root)?;
            }

            // Spend the rest of the frame budget asleep. Polling never blocks,
            // so without this the loop would busy-wait on an idle terminal.
            thread::sleep(FRAME_INTERVAL.saturating_sub(frame_start.elapsed()));
        }
    }

    // ── Helpers ──────────────────────────────────────────────────────────────
    // //

    /// Returns `true` for the built-in quit bindings (Ctrl-C, Ctrl-D).
    ///
    /// Applications that want different quit behaviour should intercept the
    /// event in [`App::handle_event`] and return `true` from there.
    fn is_quit_event(ev: &Event) -> bool {
        match *ev {
            Event::KeyPress(key) => {
                matches!(key.code, KeyCode::Char('c') | KeyCode::Char('d')) && key.modifiers == KeyModifiers::CONTROL
            },
            Event::ChannelReady => false,
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
    use std::collections::VecDeque;

    use ratatui::{Terminal, backend::TestBackend, layout::Rect, style::Style};
    use termoxide_event::event::KeyEvent;

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
            matches!(event, Event::KeyPress(KeyEvent { code: KeyCode::Char('x'), .. }))
        }
    }

    /// Number of empty polls tolerated before the fake source forces a quit.
    ///
    /// Without this the loop would spin forever on a test that forgets to queue
    /// a terminating event; here it ends instead with a wrong `build_count`,
    /// which is a readable failure rather than a hung suite.
    const MAX_IDLE_POLLS: usize = 64;

    /// Replays queued events, one per poll, so each event lands on its own
    /// frame — mirroring how a user types rather than a burst arriving at once.
    struct ScriptedEvents {
        queue: VecDeque<Event>,
        idle_polls: usize,
    }

    impl ScriptedEvents {
        fn new(events: Vec<Event>) -> Self { Self { queue: events.into(), idle_polls: 0 } }
    }

    impl EventSource for ScriptedEvents {
        fn poll_events(&mut self) -> Vec<Event> {
            match self.queue.pop_front() {
                Some(event) => vec![event],
                None => {
                    self.idle_polls += 1;
                    if self.idle_polls > MAX_IDLE_POLLS {
                        vec![make_key_event(KeyCode::Char('c'), KeyModifiers::CONTROL)]
                    } else {
                        Vec::new()
                    }
                },
            }
        }
    }

    fn make_key_event(code: KeyCode, modifiers: KeyModifiers) -> Event {
        Event::KeyPress(KeyEvent::new(code, modifiers))
    }

    fn make_loop(events: Vec<Event>) -> RenderLoop<TestBackend> {
        let backend = TestBackend::new(10, 1);
        let terminal = Terminal::new(backend).expect("terminal");
        let renderer = Renderer::new_for_test(terminal);

        RenderLoop::new(renderer, EventRouter::new(), ScriptedEvents::new(events))
    }

    #[test]
    fn run_quits_on_ctrl_c_before_app_handle() {
        let mut render_loop = make_loop(vec![make_key_event(KeyCode::Char('c'), KeyModifiers::CONTROL)]);

        let mut app = CountingApp::new();
        let result = render_loop.run(&mut app);

        assert!(result.is_ok());
        assert_eq!(app.build_count, 1);
        assert_eq!(app.handle_count, 0);
    }

    #[test]
    fn run_quits_on_ctrl_d() {
        let mut render_loop = make_loop(vec![make_key_event(KeyCode::Char('d'), KeyModifiers::CONTROL)]);

        let mut app = CountingApp::new();
        let result = render_loop.run(&mut app);

        assert!(result.is_ok());
        assert_eq!(app.handle_count, 0);
    }

    #[test]
    fn bare_c_is_not_a_quit_event() {
        // Guards the whole modifier chain: without modifiers travelling with
        // the key code, a plain 'c' would be indistinguishable from Ctrl-C.
        let mut render_loop = make_loop(vec![
            make_key_event(KeyCode::Char('c'), KeyModifiers::NONE),
            make_key_event(KeyCode::Char('c'), KeyModifiers::CONTROL),
        ]);

        let mut app = CountingApp::new();
        let result = render_loop.run(&mut app);

        assert!(result.is_ok());
        assert_eq!(app.handle_count, 1, "the bare 'c' should have reached the app");
    }

    #[test]
    fn run_stops_when_app_handles_event() {
        let mut render_loop = make_loop(vec![make_key_event(KeyCode::Char('x'), KeyModifiers::NONE)]);

        let mut app = CountingApp::new();
        let result = render_loop.run(&mut app);

        assert!(result.is_ok());
        assert_eq!(app.build_count, 1);
        assert_eq!(app.handle_count, 1);
    }

    #[test]
    fn loop_body_rebuilds_after_non_quit_event() {
        let mut render_loop = make_loop(vec![
            make_key_event(KeyCode::Char('a'), KeyModifiers::NONE),
            make_key_event(KeyCode::Char('c'), KeyModifiers::CONTROL),
        ]);

        let mut app = CountingApp::new();
        let result = render_loop.run(&mut app);

        assert!(result.is_ok());
        assert_eq!(app.build_count, 2);
        assert_eq!(app.handle_count, 1);
    }

    #[test]
    fn idle_polls_do_not_rebuild_the_view() {
        // Three empty frames go by before the quit event: none of them may
        // trigger a rebuild, otherwise the loop repaints on every tick.
        let mut render_loop = make_loop(Vec::new());
        render_loop.event_source = Box::new(ScriptedEvents { queue: VecDeque::new(), idle_polls: MAX_IDLE_POLLS - 3 });

        let mut app = CountingApp::new();
        let result = render_loop.run(&mut app);

        assert!(result.is_ok());
        assert_eq!(app.build_count, 1, "only the initial render should have happened");
        assert_eq!(app.handle_count, 0);
    }

    #[test]
    fn channel_ready_reaches_the_app_without_quitting() {
        let mut render_loop = make_loop(vec![
            Event::ChannelReady,
            make_key_event(KeyCode::Char('c'), KeyModifiers::CONTROL),
        ]);

        let mut app = CountingApp::new();
        let result = render_loop.run(&mut app);

        assert!(result.is_ok());
        assert_eq!(app.handle_count, 1);
    }
}
