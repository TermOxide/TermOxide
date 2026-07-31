//! Main application loop: waits for signals or events, drives redraws.
//!
//! [`RenderLoop`] is the beating heart of a TermOxide application.  It owns
//! the [`Renderer`][crate::renderer::Renderer] and the
//! [`EventRouter`][crate::event_router::EventRouter], and orchestrates the
//! complete **dirty → layout → render → diff → output** cycle described in
//! *Flux 2 — Cycle de rendu complet*.
//!
//! ## High-level cycle
//!
//! ```text
//! ┌──────────────────────────────────────────────────────┐
//! │                     RenderLoop::run()                │
//! │                                                      │
//! │  ┌─────────────┐      ┌──────────────────────────┐  │
//! │  │  crossterm  │      │  dirty channel           │  │
//! │  │  event      │      │  (from reactive signals) │  │
//! │  └──────┬──────┘      └──────────┬───────────────┘  │
//! │         │   select! (crossbeam)  │                  │
//! │         └────────────┬───────────┘                  │
//! │                      ▼                              │
//! │          ┌───────────────────────┐                  │
//! │          │  crossterm event?     │──── yes ────────►│
//! │          │  EventRouter::route() │                  │
//! │          └───────────────────────┘                  │
//! │                      │                              │
//! │          ┌───────────▼───────────┐                  │
//! │          │  is_subtree_dirty?    │─── no ──────────►│
//! │          └───────────────────────┘  (skip frame)    │
//! │                      │ yes                          │
//! │          ┌───────────▼───────────┐                  │
//! │          │  app.build_view()     │  component layer │
//! │          └───────────────────────┘                  │
//! │                      │                              │
//! │          ┌───────────▼───────────┐                  │
//! │          │  layout engine        │  taffy           │
//! │          └───────────────────────┘                  │
//! │                      │                              │
//! │          ┌───────────▼───────────┐                  │
//! │          │  Renderer::render_    │                  │
//! │          │       frame()         │                  │
//! │          └───────────────────────┘                  │
//! │                      │                              │
//! │              (continue loop)                        │
//! └──────────────────────────────────────────────────────┘
//! ```
//!
//! ## Reactive dirty notifications
//!
//! Rather than polling signals on every loop iteration, the reactive layer
//! sends a unit value on a [`crossbeam_channel::Sender<()>`] whenever a
//! computed value or effect that feeds the view becomes stale.  The loop
//! blocks on `select!` across the dirty channel and the crossterm event
//! channel, so it has **zero CPU cost when nothing changes**.
//!
//! The channel is deliberately a *bounded* channel of capacity 1 (a
//! "rendezvous" semaphore).  A burst of rapid signal changes collapses into a
//! single wakeup, preventing frame storms.
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

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use crossbeam_channel::{Receiver, Sender, TryRecvError, bounded, select};
use crossterm::event::{Event, KeyModifiers};
use ratatui::{backend::Backend, layout::Rect};

use crate::{
    event_router::EventRouter,
    input::{DEFAULT_POLL_TIMEOUT, poll_crossterm_events},
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
// //  DirtySignal
// ───────────────────────────────────────────────────────────────────────────
// //

/// A pair of channels used to signal that the view is stale.
///
/// The reactive layer holds the [`Sender`] and calls
/// [`DirtySignal::mark`][DirtySender::mark] inside a reactive `effect`, which
/// fires whenever a source signal changes.  The render loop holds the
/// [`Receiver`] and unblocks as soon as a dirty notification arrives.
///
/// Because the channel is bounded at capacity 1, a rapid burst of signal
/// changes collapses into a single wakeup — no frame storms.
///
/// # Example
///
/// ```rust
/// use termoxide_rendering::render_loop::dirty_channel;
///
/// let (tx, rx) = dirty_channel();
/// tx.mark(); // Signal is dirty!
/// assert!(rx.try_recv());
/// ```
pub struct DirtySender(Sender<()>);

/// See [`DirtySender`].
pub struct DirtyReceiver(Receiver<()>);

impl DirtySender {
    /// Notify the render loop that the view is stale.
    ///
    /// A second call before the render loop wakes is a no-op (the channel is
    /// bounded at 1 and the old message is still pending).
    pub fn mark(&self) {
        // Ignore send errors: the render loop may have already quit.
        let _ = self.0.try_send(());
    }
}

impl DirtyReceiver {
    /// Non-blocking check — returns `true` if a dirty notification is pending.
    pub fn is_dirty(&self) -> bool {
        matches!(self.0.try_recv(), Ok(()) | Err(TryRecvError::Disconnected))
    }

    /// Consume a pending dirty notification, returning `true` if one was
    /// present.
    pub fn try_recv(&self) -> bool { self.0.try_recv().is_ok() }

    /// Borrow the inner [`Receiver`] so it can be used in a `select!` macro.
    pub(crate) fn inner(&self) -> &Receiver<()> { &self.0 }
}

/// Create a linked [`DirtySender`] / [`DirtyReceiver`] pair.
///
/// Distribute the sender to the reactive layer and keep the receiver inside
/// the [`RenderLoop`].
pub fn dirty_channel() -> (DirtySender, DirtyReceiver) {
    let (tx, rx) = bounded(1);
    (DirtySender(tx), DirtyReceiver(rx))
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
/// use termoxide_rendering::{
///     event_router::EventRouter,
///     render_loop::{RenderLoop, dirty_channel},
///     renderer::Renderer,
/// };
///
/// let backend = CrosstermBackend::new(stdout());
/// let terminal = Terminal::new(backend).unwrap();
/// let renderer = Renderer::new(terminal).unwrap();
///
/// let (dirty_tx, dirty_rx) = dirty_channel();
/// let event_router = EventRouter::new();
///
/// // Pass dirty_tx to your reactive layer here.
///
/// // let mut app = MyApp::new();
/// // RenderLoop::new(renderer, event_router, dirty_rx).run(&mut app);
/// ```
pub struct RenderLoop<B: Backend> {
    renderer: Renderer<B>,
    event_router: EventRouter,
    dirty_rx: DirtyReceiver,
}

impl<B: Backend> RenderLoop<B> {
    /// Create a new render loop.
    ///
    /// - `renderer` — the configured [`Renderer`] to paint frames.
    /// - `event_router` — the [`EventRouter`] that maps raw crossterm events to
    ///   component ids.
    /// - `dirty_rx` — the dirty receiver produced by [`dirty_channel`].
    pub fn new(
        renderer: Renderer<B>,
        event_router: EventRouter,
        dirty_rx: DirtyReceiver,
    ) -> Self {
        Self {
            renderer,
            event_router,
            dirty_rx,
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
        // Channel for crossterm events polled from a background thread.
        let (ev_tx, ev_rx) = bounded::<Event>(64);
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_worker = Arc::clone(&stop_flag);

        // Spawn a thread that polls crossterm events and forwards batches
        // through the channel.
        let ev_thread = std::thread::spawn(move || {
            while !stop_flag_worker.load(Ordering::Relaxed) {
                match poll_crossterm_events(DEFAULT_POLL_TIMEOUT) {
                    Ok(events) => {
                        for ev in events {
                            if ev_tx.send(ev).is_err() {
                                // Receiver dropped — render loop has quit.
                                return;
                            }
                        }
                    },
                    Err(_) => return,
                }
            }
        });

        let result = self.loop_body(app, &ev_rx);

        // Restore terminal unconditionally.
        let restore_result = self.renderer.restore();

        // Stop and join the event thread.
        stop_flag.store(true, Ordering::Relaxed);
        drop(ev_rx); // Disconnect the channel so the thread exits.
        let _ = ev_thread.join();

        restore_result?;
        result
    }

    // ── Inner loop ───────────────────────────────────────────────────────────
    // //

    fn loop_body<A: App>(
        &mut self,
        app: &mut A,
        ev_rx: &Receiver<Event>,
    ) -> Result<(), RenderLoopError> {
        // Force an initial render.
        let viewport = self.renderer.viewport();
        let mut root = app.build_view(viewport);
        self.renderer.render_frame(&mut root)?;
        // Build initial spatial index so mouse events are routed immediately.
        self.event_router.sync_hit_map(&root);

        loop {
            // Block until either an event arrives or a dirty notification
            // fires.
            select! {
                recv(ev_rx) -> msg => {
                    let ev = msg.map_err(|_| RenderLoopError::EventThreadDied)?;

                    // Default quit bindings: Ctrl-C and Esc.
                    if Self::is_quit_event(&ev) {
                        return Ok(());
                    }

                    // Route the event to the appropriate component.
                    let target_id = self.event_router.route_event(&ev, &root);

                    // Let the application handle it.
                    if app.handle_event(target_id, ev) {
                        return Ok(());
                    }

                    // The event may have dirtied some signals — also consume
                    // a pending dirty-channel notification so we don't wait
                    // for the next select! iteration.
                    if root.is_subtree_dirty() || self.dirty_rx.try_recv() {
                        let viewport = self.renderer.viewport();
                        root = app.build_view(viewport);
                        self.event_router.sync_hit_map(&root);
                        self.renderer.render_frame(&mut root)?;
                    }
                }

                recv(self.dirty_rx.inner()) -> _ => {
                    // A reactive signal fired — rebuild the view tree.
                    let viewport = self.renderer.viewport();
                    root = app.build_view(viewport);
                    self.event_router.sync_hit_map(&root);
                    self.renderer.render_frame(&mut root)?;
                }
            }
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
    /// The background event-reading thread exited unexpectedly.
    EventThreadDied,
}

impl std::fmt::Display for RenderLoopError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Render(e) => write!(f, "render error: {e}"),
            Self::EventThreadDied => {
                write!(f, "crossterm event thread exited unexpectedly")
            },
        }
    }
}

impl std::error::Error for RenderLoopError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Render(e) => Some(e),
            Self::EventThreadDied => None,
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
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState};
    use ratatui::backend::TestBackend;

    use super::*;

    fn key_event(code: KeyCode, modifiers: KeyModifiers) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    #[test]
    fn dirty_channel_coalesces_multiple_marks() {
        let (tx, rx) = dirty_channel();

        tx.mark();
        tx.mark();
        tx.mark();

        assert!(rx.try_recv());
        assert!(!rx.try_recv());
    }

    #[test]
    fn is_dirty_consumes_pending_signal() {
        let (tx, rx) = dirty_channel();
        tx.mark();

        assert!(rx.is_dirty());
        assert!(!rx.is_dirty());
    }

    #[test]
    fn disconnected_dirty_sender_is_treated_as_dirty() {
        let (tx, rx) = dirty_channel();
        drop(tx);

        assert!(rx.is_dirty());
    }

    #[test]
    fn quit_event_detects_ctrl_c_and_ctrl_d_only() {
        let ctrl_c = key_event(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let ctrl_d = key_event(KeyCode::Char('d'), KeyModifiers::CONTROL);
        let plain_c = key_event(KeyCode::Char('c'), KeyModifiers::NONE);
        let ctrl_shift_c = key_event(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        );

        assert!(RenderLoop::<TestBackend>::is_quit_event(&ctrl_c));
        assert!(RenderLoop::<TestBackend>::is_quit_event(&ctrl_d));
        assert!(!RenderLoop::<TestBackend>::is_quit_event(&plain_c));
        assert!(!RenderLoop::<TestBackend>::is_quit_event(&ctrl_shift_c));
    }
}
