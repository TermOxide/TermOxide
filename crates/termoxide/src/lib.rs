//! TermOxide's application loop.
//!
//! [`run_with_app`] owns the reactive runtime, terminal, and input reader while
//! driving the input, update, build, and render cycle. Reactive changes request
//! repaints; [`MIN_FRAME`] limits their frequency while [`TICK_INTERVAL`] drives
//! time-based updates.

mod capture;
mod error;

use std::{
    cell::Cell,
    io::stdout,
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use capture::Guard;
use color_eyre::Result;
use ratatui::{
    Terminal,
    backend::{Backend, CrosstermBackend},
    layout::Rect,
};
use reactive_graph::effect::RenderEffect;
use termoxide_event::{EventStream, EventStreamConfig, event::Event};
use termoxide_rendering::{renderer::Renderer, view_node::ViewNode};
// Same as `std::time::Instant`, but follows tokio's paused clock in tests.
use tokio::time::Instant;

pub use crate::error::{AppMethod, AppPanic, LoopError, LoopFailure, PanicLocation, Trace, TraceFrame};

/// Minimum time between repaints.
pub const MIN_FRAME: Duration = Duration::from_millis(16);
/// How often terminal input is drained.
pub const INPUT_POLL: Duration = Duration::from_millis(8);
/// How often [`App::on_tick`] fires.
pub const TICK_INTERVAL: Duration = Duration::from_millis(100);

/// Application callbacks used by [`run_with_app`].
pub trait App {
    /// Read the signals that should trigger a view rebuild.
    fn track_view(&self);

    /// Advance time-based state.
    fn on_tick(&self);

    /// Handle an input event. Return `true` to stop the loop.
    fn handle_event(&self, event: Event) -> bool;

    /// Build the view tree for `viewport` when a repaint is due.
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

/// Non-blocking input source used by the application loop.
pub trait EventSource {
    /// Return currently available events in oldest-first order.
    fn poll_events(&self) -> Vec<Event>;

    /// Stop the source once the loop is over.
    ///
    /// # Errors
    ///
    /// Returns the error the source stopped on, if any.
    fn teardown(self) -> termoxide_event::Result<()>
    where
        Self: Sized;
}

impl EventSource for EventStream {
    fn poll_events(&self) -> Vec<Event> { EventStream::poll_events(self) }

    fn teardown(self) -> termoxide_event::Result<()> { EventStream::teardown(self) }
}

/// What the render effect hands to the loop.
///
/// The flag and the wakeup are separate on purpose: [`Notify`](tokio::sync::Notify)
/// alone would force the loop to `await` a notification to learn a repaint is
/// due, but the effect runs on the executor *after* the loop has already come
/// back from `select!`. Storing the request in an [`AtomicBool`] lets the loop
/// pick it up in the same iteration; the notify only wakes a loop that is idle.
#[derive(Default)]
struct Redraw {
    requested: AtomicBool,
    wake: tokio::sync::Notify,
    /// The first panic of `track_view`: the effect's re-runs happen in a task
    /// of their own, where a panic would only kill that task.
    panic: Cell<Option<AppPanic>>,
}

impl Redraw {
    /// Request a repaint and wake an idle loop.
    fn request(&self) {
        self.requested.store(true, Ordering::Release);
        self.wake.notify_one();
    }

    /// Take the pending request, if any.
    fn take(&self) -> bool { self.requested.swap(false, Ordering::AcqRel) }

    /// Keep `panic` for the loop, unless an earlier one is already waiting.
    fn report_panic(&self, panic: AppPanic) {
        let first = self.panic.take().unwrap_or(panic);
        self.panic.set(Some(first));
        self.request();
    }

    fn take_panic(&self) -> Option<AppPanic> { self.panic.take() }
}

/// Tracks whether a repaint is dirty and whether its frame budget has elapsed.
#[derive(Debug)]
struct FramePacer {
    min_frame: Duration,
    last_draw: Instant,
    dirty: bool,
}

impl FramePacer {
    /// Start dirty so the first frame draws immediately.
    fn new(now: Instant, min_frame: Duration) -> Self {
        Self {
            min_frame,
            // Backdate the first frame without assuming `Instant` can subtract.
            last_draw: now.checked_sub(min_frame).unwrap_or(now),
            dirty: true,
        }
    }

    fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Whether a dirty frame is ready to draw.
    fn should_draw(&self, now: Instant) -> bool {
        self.dirty && now.duration_since(self.last_draw) >= self.min_frame
    }

    fn record_draw(&mut self, now: Instant) {
        self.dirty = false;
        self.last_draw = now;
    }
}

/// The input settings `app` asks for.
fn stream_config<A: App>(app: &A) -> EventStreamConfig {
    EventStreamConfig::default().mouse_capture(app.mouse_capture())
}

/// Hand every pending event to the app, stopping at the first quit request or
/// panic.
///
/// Returns `true` when the app asked to quit.
fn pump_events<A: App, E: EventSource>(app: &A, events: &E, guard: Guard) -> Result<bool, AppPanic> {
    for event in events.poll_events() {
        if guard.call(AppMethod::HandleEvent, || app.handle_event(event))? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Run `app` on the real terminal until it requests shutdown.
///
/// # Errors
///
/// Returns an error if the terminal cannot be set up. Any later failure stops
/// the loop, restores the terminal, and is returned as a [`LoopError`]: a frame
/// that failed to render, a panic in an [`App`] method, the input reader having
/// stopped on an error, or a loop failure together with a reader failure.
///
/// # Panics
///
/// A panic in an [`App`] method is returned as [`LoopFailure::Panic`] instead
/// of unwinding, with its location and, per [`Trace`], the app's stack. This
/// relies on unwinding (not `panic = "abort"`) and on a panic hook the first
/// call installs for the whole process, which forwards every other panic to
/// the previous hook: install your own hooks, such as `color_eyre::install()`,
/// before calling this.
pub async fn run_with_app<A: App + Clone + 'static>(app: A) -> Result<()> {
    let events = EventStream::with_config(stream_config(&app));

    let terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let renderer = Renderer::new(terminal)?;

    run_with(app, renderer, events, capture::trace_from_env()).await
}

/// [`run_with_app`] on any backend and event source, with the stack trace
/// policy given rather than read from the environment. For TermOxide's tests.
#[cfg(feature = "test-util")]
#[doc(hidden)]
pub async fn run_with_backend<A, B, E>(app: A, renderer: Renderer<B>, events: E, trace: bool) -> Result<()>
where
    A: App + Clone + 'static,
    B: Backend,
    E: EventSource,
{
    run_with(app, renderer, events, trace).await
}

async fn run_with<A, B, E>(app: A, mut renderer: Renderer<B>, events: E, trace: bool) -> Result<()>
where
    A: App + Clone + 'static,
    B: Backend,
    E: EventSource,
{
    capture::install_hook();
    let guard = Guard::new(trace);

    let owner = termoxide_reactive::Owner::new();
    owner.set();

    let redraw = Rc::new(Redraw::default());
    let _redraw_effect = {
        let app_for_effect = app.clone();
        let redraw = Rc::clone(&redraw);
        RenderEffect::new(move |_| {
            match guard.call(AppMethod::TrackView, || app_for_effect.track_view()) {
                Ok(()) => redraw.request(),
                Err(panic) => redraw.report_panic(panic),
            }
        })
    };

    // The effect's first run happens inside `RenderEffect::new`.
    let failure = match redraw.take_panic() {
        Some(panic) => Some(LoopFailure::Panic(panic)),
        None => drive(&app, &mut renderer, &events, &redraw, guard).await.err(),
    };

    // Restore the terminal before reporting: the reader thread owns raw mode,
    // and its own failure is a likely reason the loop stopped in the first
    // place, so it is reported alongside the loop's rather than discarded.
    let teardown = events.teardown().err();
    match LoopError::from_parts(failure, teardown) {
        Some(error) => Err(error.into()),
        None => Ok(()),
    }
}

/// The loop proper, generic over the backend and the event source so it can be
/// driven without a terminal.
async fn drive<A, B, E>(
    app: &A,
    renderer: &mut Renderer<B>,
    events: &E,
    redraw: &Redraw,
    guard: Guard,
) -> Result<(), LoopFailure>
where
    A: App,
    B: Backend,
    E: EventSource,
{
    let mut pacer = FramePacer::new(Instant::now(), MIN_FRAME);
    let mut viewport = renderer.viewport();

    let mut ticker = tokio::time::interval(TICK_INTERVAL);
    let mut input = tokio::time::interval(INPUT_POLL);
    // Avoid bursts of catch-up work after a slow frame.
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    input.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = input.tick() => {
                if pump_events(app, events, guard)? {
                    return Ok(());
                }
            }
            _ = ticker.tick() => {
                guard.call(AppMethod::OnTick, || app.on_tick())?;

                // Resizes are observed by polling the terminal viewport.
                let current = renderer.viewport();
                if current != viewport {
                    viewport = current;
                    pacer.mark_dirty();
                }
            }
            _ = redraw.wake.notified() => {}
        }

        // Let scheduled render effects run before checking for repaint work.
        tokio::task::yield_now().await;

        if let Some(panic) = redraw.take_panic() {
            return Err(LoopFailure::Panic(panic));
        }

        if redraw.take() {
            pacer.mark_dirty();
        }

        if pacer.should_draw(Instant::now()) {
            let mut root = guard.call(AppMethod::BuildView, || app.build_view(viewport))?;
            renderer.render_frame(&mut root).map_err(LoopFailure::Render)?;
            pacer.record_draw(Instant::now());
        }
    }
}

#[cfg(test)]
mod tests;
