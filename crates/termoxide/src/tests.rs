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

    fn teardown(self) -> termoxide_event::Result<()> { Ok(()) }
}

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
