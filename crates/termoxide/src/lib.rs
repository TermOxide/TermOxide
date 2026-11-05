//! The TermOxide application entry point.
//!
//! [`run_with_app`] owns the process-level plumbing an application needs — the
//! reactive [`Owner`](termoxide_reactive::Owner), the terminal, the input
//! reader thread — and drives the **input → update → build → render** cycle
//! until the application asks to stop.
//!
//! ## Cadence
//!
//! Three independent rhythms share one `select!`:
//!
//! | Rhythm            | Period          | Purpose                                       |
//! |-------------------|-----------------|-----------------------------------------------|
//! | [`INPUT_POLL`]    | 8 ms            | drain terminal input; bounds input latency    |
//! | [`TICK_INTERVAL`] | 100 ms          | fire [`App::on_tick`]; notice terminal resizes |
//! | redraw request    | on signal write | a tracked signal changed, so repaint          |
//!
//! Repainting is driven by the reactive layer, not by the clock: a
//! [`RenderEffect`](reactive_graph::effect::RenderEffect) re-runs [`App::track_view`] whenever a signal it read
//! changes, and requests a redraw. An idle application therefore draws nothing,
//! and [`MIN_FRAME`] caps how often a busy one can draw.

#[cfg(feature = "terminal")]
mod capture;
#[cfg(feature = "terminal")]
mod error;
#[cfg(feature = "terminal")]
mod run;

use ratatui::layout::Rect;
use termoxide_event::event::Event;
use termoxide_rendering::view_node::ViewNode;

#[cfg(feature = "test-util")]
#[doc(hidden)]
pub use crate::run::run_with_backend;
#[cfg(feature = "terminal")]
pub use crate::{
    error::{AppMethod, AppPanic, LoopError, LoopFailure, PanicLocation, Trace, TraceFrame},
    run::{EventSource, INPUT_POLL, MIN_FRAME, TICK_INTERVAL, run_with_app},
};

/// The contract an application implements to be driven by [`run_with_app`].
///
/// Every method takes `&self`: state lives in reactive signals, which are
/// interior-mutable, so the loop never needs a unique borrow.
pub trait App {
    /// Read every signal the view depends on.
    ///
    /// Called inside the loop's render effect, so reading a signal here subscribes
    /// the loop to it: any later write requests a repaint. Read, and discard —
    /// the values themselves are fetched again in [`build_view`](Self::build_view).
    fn track_view(&self);

    /// Advance time-based state. Fires every [`TICK_INTERVAL`].
    fn on_tick(&self);

    /// Handle one input event.
    ///
    /// Returns `true` to stop the loop. Events queued behind a `true` are
    /// dropped undelivered.
    fn handle_event(&self, event: Event) -> bool;

    /// Build the view tree for `viewport`.
    ///
    /// Called only on frames that actually repaint.
    fn build_view(&self, viewport: Rect) -> ViewNode;

    /// Whether the terminal reports the mouse while the app runs.
    ///
    /// On by default, so the app receives mouse events in
    /// [`handle_event`](Self::handle_event). While it is on, the
    /// terminal stops handling text selection itself — most terminals still
    /// select with `Shift` held — so an app that ignores the mouse can return
    /// `false` to give selection back. Read once, before the loop starts.
    fn mouse_capture(&self) -> bool { true }
}
