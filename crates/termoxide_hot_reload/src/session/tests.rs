#![allow(clippy::unwrap_used, clippy::panic)]

use std::future::Future;

use any_spawner::Executor;
use ratatui::{layout::Rect, style::Style};
use serde::{Deserialize, Serialize};
use termoxide_event::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use termoxide_reactive::Signal;
use termoxide_rendering::view_node::{ViewContent, ViewNode};

use super::*;

/// The view shows `shown`; `hidden` is state the view never reads.
#[derive(Clone, Copy, Serialize, Deserialize)]
struct Counter {
    shown: Signal<u32>,
    hidden: Signal<u32>,
}

impl Counter {
    fn new() -> Self { Self { shown: Signal::new(0), hidden: Signal::new(0) } }
}

impl App for Counter {
    fn track_view(&self) { let _ = self.shown.get(); }

    fn on_tick(&self) { self.shown.update(|n| *n += 1); }

    fn handle_event(&self, event: Event) -> bool {
        match event {
            Event::KeyPress(key) if key.code == KeyCode::Char('h') => {
                self.hidden.update(|n| *n += 1);
                false
            },
            Event::KeyPress(key) => key.code == KeyCode::Char('q'),
            _ => false,
        }
    }

    fn build_view(&self, viewport: Rect) -> ViewNode {
        ViewNode::text(viewport, self.shown.get_untracked().to_string(), Style::default())
    }

    fn mouse_capture(&self) -> bool { false }
}

fn key(c: char) -> Request { Request::Event(Event::KeyPress(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))) }

/// Runs `test` the way the subprocess runs: a current-thread runtime and a
/// local set the reactive effects are spawned on.
fn run(test: impl Future<Output = ()>) {
    let _ = Executor::init_tokio();
    let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap();
    tokio::task::LocalSet::new().block_on(&runtime, test);
}

/// A responder whose app has started from scratch.
async fn started() -> Responder<Counter> {
    let mut responder = Responder::new(Counter::new());
    answer(&mut responder, Request::Start { state: None }).await;
    responder
}

/// Answers `request` the way the subprocess does: letting the render effect
/// run before settling.
async fn answer(responder: &mut Responder<Counter>, request: Request) -> Response {
    match responder.step(request).unwrap() {
        Step::Reply(response) => response,
        Step::Settle { quit } => {
            tokio::task::yield_now().await;
            responder.settled(quit).unwrap()
        },
    }
}

/// The state of a `Counter` showing `shown` and hiding `hidden`, as the host
/// keeps it: the type's fingerprint, then the app.
fn state(shown: u32, hidden: u32) -> Vec<u8> {
    let mut state = schema::fingerprint::<Counter>().unwrap().to_le_bytes().to_vec();
    state.extend(postcard::to_allocvec(&(shown, hidden)).unwrap());
    state
}

fn outcome(response: Response) -> Outcome {
    match response {
        Response::Done(outcome) => outcome,
        other => panic!("expected an outcome, got {other:?}"),
    }
}

#[test]
fn start_reports_mouse_capture_and_the_state() {
    run(async {
        let mut responder = Responder::new(Counter::new());

        let started = answer(&mut responder, Request::Start { state: None }).await;

        let Response::Started { mouse_capture, state: started, reset } = started else {
            panic!("expected Started")
        };
        assert!(!mouse_capture);
        assert_eq!(started, state(0, 0));
        assert!(!reset);
    });
}

#[test]
fn a_change_the_view_reads_is_dirty_and_sends_the_state() {
    run(async {
        let mut responder = started().await;

        let done = outcome(answer(&mut responder, Request::Tick).await);

        assert!(done.dirty);
        assert_eq!(done.state, Some(state(1, 0)));
    });
}

#[test]
fn a_change_the_view_doesnt_read_sends_the_state_without_repainting() {
    run(async {
        let mut responder = started().await;

        let done = outcome(answer(&mut responder, key('h')).await);

        assert!(!done.dirty);
        assert_eq!(done.state, Some(state(0, 1)));
    });
}

#[test]
fn an_event_that_changes_nothing_sends_nothing() {
    run(async {
        let mut responder = started().await;

        let done = outcome(answer(&mut responder, key('x')).await);

        assert_eq!(done, Outcome { quit: false, dirty: false, state: None });
    });
}

#[test]
fn the_state_is_only_sent_again_once_it_changes_again() {
    run(async {
        let mut responder = started().await;

        assert!(outcome(answer(&mut responder, Request::Tick).await).state.is_some());
        assert_eq!(outcome(answer(&mut responder, key('x')).await).state, None);
        assert!(outcome(answer(&mut responder, Request::Tick).await).state.is_some());
    });
}

#[test]
fn a_quit_event_is_reported() {
    run(async {
        let mut responder = started().await;

        assert!(outcome(answer(&mut responder, key('q')).await).quit);
    });
}

#[test]
fn a_session_resumes_from_the_hosts_state() {
    run(async {
        let state = state(7, 3);
        let mut responder = Responder::new(Counter::new());
        answer(&mut responder, Request::Start { state: Some(state) }).await;

        let Response::View(view) = answer(&mut responder, Request::BuildView(Rect::new(0, 0, 5, 1))).await else {
            panic!("expected a view")
        };
        assert!(matches!(view.content, ViewContent::Text { ref text, .. } if text == "7"));
    });
}

#[test]
fn a_second_start_is_refused() {
    run(async {
        let mut responder = started().await;

        let Err(error) = responder.step(Request::Start { state: None }) else {
            panic!("a second start was accepted")
        };

        assert!(matches!(error, IpcError::Unexpected), "{error}");
    });
}

#[test]
fn only_a_ping_or_a_start_is_answered_before_the_app_starts() {
    run(async {
        let mut responder = Responder::new(Counter::new());

        assert!(matches!(answer(&mut responder, Request::Ping).await, Response::Pong));
        assert!(matches!(responder.step(Request::Tick), Err(IpcError::Unexpected)));
    });
}

/// Starts a `Counter` from `saved`, returning whether it reset and the view.
async fn start_from(saved: Vec<u8>) -> (bool, String) {
    let mut responder = Responder::new(Counter::new());
    let Response::Started { reset, .. } = answer(&mut responder, Request::Start { state: Some(saved) }).await else {
        panic!("expected Started")
    };
    let Response::View(view) = answer(&mut responder, Request::BuildView(Rect::new(0, 0, 5, 1))).await else {
        panic!("expected a view")
    };
    let ViewContent::Text { text, .. } = view.content else {
        panic!("expected text")
    };
    (reset, text)
}

#[test]
fn a_state_saved_by_another_state_type_starts_the_app_afresh() {
    run(async {
        let mut saved = state(7, 3);
        saved[0] ^= 1;

        assert_eq!(start_from(saved).await, (true, String::from("0")));
    });
}

#[test]
fn a_state_too_short_to_hold_a_fingerprint_starts_the_app_afresh() {
    run(async { assert_eq!(start_from(vec![1, 2, 3]).await, (true, String::from("0"))) });
}

#[test]
fn a_state_of_the_right_type_that_does_not_decode_is_an_error() {
    run(async {
        let mut saved = state(7, 3);
        saved.truncate(9);
        let mut responder = Responder::new(Counter::new());

        let Err(error) = responder.step(Request::Start { state: Some(saved) }) else {
            panic!("a corrupt state was accepted")
        };

        assert!(matches!(error, IpcError::Decode(_)), "{error}");
    });
}
