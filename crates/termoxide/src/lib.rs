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
//! [`RenderEffect`] re-runs [`App::track_view`] whenever a signal it read
//! changes, and requests a redraw. An idle application therefore draws nothing,
//! and [`MIN_FRAME`] caps how often a busy one can draw.

use std::{
    io::stdout,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use color_eyre::Result;
use ratatui::{
    Terminal,
    backend::{Backend, CrosstermBackend},
    layout::Rect,
};
use reactive_graph::effect::RenderEffect;
use termoxide_event::{EventStream, event::Event};
use termoxide_rendering::{renderer::Renderer, view_node::ViewNode};

/// Shortest gap between two repaints (~60 fps).
pub const MIN_FRAME: Duration = Duration::from_millis(16);
/// How often terminal input is drained.
///
/// Input latency is bounded by this, so it is deliberately far shorter than
/// [`TICK_INTERVAL`]: draining input on the tick would make every keypress wait
/// up to a full tick before the application saw it.
pub const INPUT_POLL: Duration = Duration::from_millis(8);
/// How often [`App::on_tick`] fires.
pub const TICK_INTERVAL: Duration = Duration::from_millis(100);

/// The contract an application implements to be driven by [`run_with_app`].
///
/// Every method takes `&self`: state lives in reactive signals, which are
/// interior-mutable, so the loop never needs a unique borrow.
pub trait App {
    /// Read every signal the view depends on.
    ///
    /// Called inside a [`RenderEffect`], so reading a signal here subscribes
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
}

/// A non-blocking source of input events.
///
/// Exists so the loop can be driven by a fake in tests; production uses
/// [`EventStream`].
pub trait EventSource {
    /// Return every event available right now, oldest first. Must not block.
    fn poll_events(&self) -> Vec<Event>;
}

impl EventSource for EventStream {
    fn poll_events(&self) -> Vec<Event> { EventStream::poll_events(self) }
}

/// A pending repaint request, set by the render effect and consumed by the loop.
///
/// The flag and the wakeup are separate on purpose: [`Notify`](tokio::sync::Notify)
/// alone would force the loop to `await` a notification to learn a repaint is
/// due, but the effect runs on the executor *after* the loop has already come
/// back from `select!`. Storing the request in an [`AtomicBool`] lets the loop
/// pick it up in the same iteration; the notify only wakes a loop that is idle.
#[derive(Debug, Default)]
struct Redraw {
    requested: AtomicBool,
    wake: tokio::sync::Notify,
}

impl Redraw {
    /// Ask for a repaint and wake the loop if it is parked.
    fn request(&self) {
        self.requested.store(true, Ordering::Release);
        self.wake.notify_one();
    }

    /// Take the pending request, if any.
    fn take(&self) -> bool { self.requested.swap(false, Ordering::AcqRel) }
}

/// Decides when the loop owes the terminal a repaint.
///
/// Pure, so the pacing rules can be tested without a terminal.
#[derive(Debug)]
struct FramePacer {
    min_frame: Duration,
    last_draw: Instant,
    dirty: bool,
}

impl FramePacer {
    /// Start dirty, so the first frame is drawn immediately.
    fn new(now: Instant, min_frame: Duration) -> Self {
        Self {
            min_frame,
            // Backdate so the first frame is not held for `min_frame`.
            // `checked_sub` because `now` can be close to the platform epoch.
            last_draw: now.checked_sub(min_frame).unwrap_or(now),
            dirty: true,
        }
    }

    fn mark_dirty(&mut self) { self.dirty = true; }

    /// `true` when something changed *and* the frame budget has elapsed.
    fn should_draw(&self, now: Instant) -> bool { self.dirty && now.duration_since(self.last_draw) >= self.min_frame }

    fn record_draw(&mut self, now: Instant) {
        self.dirty = false;
        self.last_draw = now;
    }
}

/// Hand every pending event to the app, stopping at the first quit request.
///
/// Returns `true` when the app asked to quit.
fn pump_events<A: App, E: EventSource>(app: &A, events: &E) -> bool {
    events.poll_events().into_iter().any(|event| app.handle_event(event))
}

/// Run `app` on the real terminal until it asks to stop.
///
/// Sets up the reactive owner, the alternate screen and the input reader, drives
/// the loop, then restores the terminal before returning.
///
/// # Errors
///
/// Returns an error if the terminal cannot be set up, if a frame fails to
/// render, or if the input reader thread stopped on an error of its own.
pub async fn run_with_app<A: App + Clone + 'static>(app: A) -> Result<()> {
    let owner = termoxide_reactive::Owner::new();
    owner.set();

    let redraw = Arc::new(Redraw::default());
    let _redraw_effect = {
        let app_for_effect = app.clone();
        let redraw = Arc::clone(&redraw);
        RenderEffect::new(move |_| {
            app_for_effect.track_view();
            redraw.request();
        })
    };

    let events = EventStream::new();

    let terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let mut renderer = Renderer::new(terminal)?;

    let result = drive(&app, &mut renderer, &events, &redraw).await;

    // Restore the terminal before reporting: the reader thread owns raw mode,
    // and its own failure is a likely reason the loop stopped in the first
    // place, so its result is worth surfacing rather than discarding.
    let teardown = events.teardown();
    result?;
    teardown?;
    Ok(())
}

/// The loop proper, generic over the backend and the event source so it can be
/// driven without a terminal.
async fn drive<A, B, E>(app: &A, renderer: &mut Renderer<B>, events: &E, redraw: &Redraw) -> Result<()>
where
    A: App,
    B: Backend,
    E: EventSource,
{
    let mut pacer = FramePacer::new(Instant::now(), MIN_FRAME);
    let mut viewport = renderer.viewport();

    let mut ticker = tokio::time::interval(TICK_INTERVAL);
    let mut input = tokio::time::interval(INPUT_POLL);
    // Never replay missed ticks: a slow frame must not queue a burst of catch-up
    // work behind it.
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    input.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = input.tick() => {
                if pump_events(app, events) {
                    return Ok(());
                }
            }
            _ = ticker.tick() => {
                app.on_tick();

                // A resize raises no event and writes no signal, so it is only
                // observable by asking the terminal.
                let current = renderer.viewport();
                if current != viewport {
                    viewport = current;
                    pacer.mark_dirty();
                }
            }
            _ = redraw.wake.notified() => {}
        }

        // `RenderEffect` re-runs are scheduled on the executor rather than run
        // inline by a signal write, so let the effect task run before asking
        // whether this iteration owes a repaint.
        tokio::task::yield_now().await;

        if redraw.take() {
            pacer.mark_dirty();
        }

        if pacer.should_draw(Instant::now()) {
            let mut root = app.build_view(viewport);
            renderer.render_frame(&mut root)?;
            pacer.record_draw(Instant::now());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use termoxide_event::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;

    fn key(c: char) -> Event { Event::KeyPress(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)) }

    /// Records what the loop handed it, and quits on a nominated key.
    struct RecordingApp {
        seen: RefCell<Vec<Event>>,
        quit_on: Option<char>,
    }

    impl RecordingApp {
        fn new(quit_on: Option<char>) -> Self { Self { seen: RefCell::new(Vec::new()), quit_on } }
    }

    impl App for RecordingApp {
        fn track_view(&self) {}

        fn on_tick(&self) {}

        fn handle_event(&self, event: Event) -> bool {
            self.seen.borrow_mut().push(event);
            match (&event, self.quit_on) {
                (Event::KeyPress(pressed), Some(quit)) => pressed.code == KeyCode::Char(quit),
                _ => false,
            }
        }

        fn build_view(&self, viewport: Rect) -> ViewNode { ViewNode::container(viewport, Vec::new()) }
    }

    struct FakeEvents(Vec<Event>);

    impl EventSource for FakeEvents {
        fn poll_events(&self) -> Vec<Event> { self.0.clone() }
    }

    // ── pump_events ──────────────────────────────────────────────────────────

    #[test]
    fn pump_events_reports_no_quit_when_nothing_is_pending() {
        let app = RecordingApp::new(Some('q'));

        assert!(!pump_events(&app, &FakeEvents(Vec::new())));
        assert!(app.seen.borrow().is_empty());
    }

    #[test]
    fn pump_events_forwards_every_event_in_order() {
        let app = RecordingApp::new(None);
        let events = FakeEvents(vec![Event::ChannelReady, key('a'), key('b')]);

        assert!(!pump_events(&app, &events));
        assert_eq!(app.seen.borrow().len(), 3);
        assert!(matches!(app.seen.borrow()[0], Event::ChannelReady));
        assert!(matches!(app.seen.borrow()[1], Event::KeyPress(k) if k.code == KeyCode::Char('a')));
        assert!(matches!(app.seen.borrow()[2], Event::KeyPress(k) if k.code == KeyCode::Char('b')));
    }

    #[test]
    fn pump_events_stops_delivering_after_a_quit_request() {
        let app = RecordingApp::new(Some('q'));
        let events = FakeEvents(vec![key('a'), key('q'), key('b')]);

        assert!(pump_events(&app, &events));
        assert_eq!(
            app.seen.borrow().len(),
            2,
            "events queued behind the quit must not be delivered"
        );
    }

    // ── FramePacer ───────────────────────────────────────────────────────────

    #[test]
    fn frame_pacer_draws_the_very_first_frame() {
        let now = Instant::now();

        assert!(FramePacer::new(now, MIN_FRAME).should_draw(now));
    }

    #[test]
    fn frame_pacer_holds_a_second_frame_inside_the_budget() {
        let now = Instant::now();
        let mut pacer = FramePacer::new(now, MIN_FRAME);

        pacer.record_draw(now);
        pacer.mark_dirty();

        assert!(!pacer.should_draw(now + MIN_FRAME / 2));
        assert!(pacer.should_draw(now + MIN_FRAME));
    }

    #[test]
    fn frame_pacer_stays_clean_until_something_marks_it_dirty() {
        let now = Instant::now();
        let mut pacer = FramePacer::new(now, MIN_FRAME);

        pacer.record_draw(now);

        // An idle application draws nothing, however much time passes.
        assert!(!pacer.should_draw(now + MIN_FRAME * 100));

        pacer.mark_dirty();
        assert!(pacer.should_draw(now + MIN_FRAME * 100));
    }

    // ── Redraw ───────────────────────────────────────────────────────────────

    #[test]
    fn redraw_request_is_taken_exactly_once() {
        let redraw = Redraw::default();

        assert!(!redraw.take());

        redraw.request();
        assert!(redraw.take());
        assert!(!redraw.take());
    }

    #[test]
    fn redraw_collapses_a_burst_into_one_repaint() {
        let redraw = Redraw::default();

        redraw.request();
        redraw.request();
        redraw.request();

        assert!(redraw.take());
        assert!(!redraw.take());
    }
}
