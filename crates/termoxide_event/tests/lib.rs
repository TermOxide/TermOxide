//! Integration tests for the [`EventStream`] lifecycle: receiving events and
//! shutting the reader thread down, both via an explicit `teardown` and via
//! `Drop`.
//!
//! Every test is marked `#[serial]` because each one puts the real terminal
//! into raw mode; running two at once would let them fight over the terminal
//! and interleave raw-mode toggling non-deterministically.

use std::{
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use serial_test::serial;
use termoxide_event::{EventStream, event::Event};

/// Block the test until the stream yields at least one event, or fail after a
/// deadline.
///
/// Unlike the old blocking `recv`, [`EventStream::poll_events`] is
/// non-blocking and returns an empty `Vec` when nothing is pending, so a
/// single call right after startup can race ahead of the reader thread's
/// `ChannelReady`. This helper restores the "wait until the stream is live"
/// barrier the lifecycle tests rely on by polling until something arrives.
fn wait_for_events(events: &EventStream) -> Vec<Event> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let batch = events.poll_events();
        if !batch.is_empty() {
            return batch;
        }
        assert!(
            Instant::now() < deadline,
            "EventStream::poll_events() never yielded an event within the \
             timeout"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[serial]
fn check_receive() {
    let events = EventStream::new();
    let received = wait_for_events(&events);
    assert_eq!(
        received.first(),
        Some(&Event::ChannelReady),
        "the first polled event should be ChannelReady"
    );
}

#[test]
#[serial]
fn handle_drop_stops_thread() {
    let events = EventStream::new();
    // Barrier: make sure the reader thread is live before we drop the handle.
    wait_for_events(&events);

    // Run the drop (which joins the reader thread) on a side thread and wait
    // on it with a timeout: if the join ever hangs, the test fails after 3s
    // instead of blocking the whole suite forever.
    let (done_tx, done_rx) = mpsc::channel();
    thread::spawn(move || {
        drop(events);
        let _ = done_tx.send(());
    });

    assert!(
        done_rx.recv_timeout(Duration::from_secs(3)).is_ok(),
        "EventHandle::drop did not complete: the thread was not stopped"
    );
}

#[test]
#[serial]
fn check_teardown_stops_thread() {
    let events = EventStream::new();
    // Barrier: make sure the reader thread is live before we tear it down.
    wait_for_events(&events);

    // Same timeout guard as `handle_drop_stops_thread`, but exercising the
    // explicit `teardown` path rather than `Drop`.
    let (done_tx, done_rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = events.teardown();
        let _ = done_tx.send(());
    });

    assert!(
        done_rx.recv_timeout(Duration::from_secs(3)).is_ok(),
        "EventHandle::teardown did not complete: the thread was not stopped"
    );
}

#[test]
#[serial]
fn drop_immediately_shuts_down_cleanly() {
    // Drop the stream right away, without ever polling. Depending on timing the
    // reader thread may or may not have sent `ChannelReady` yet, so the
    // `ChannelReady` send may succeed or fail — either way the thread must shut
    // down without panicking. Reaching the end of the test is the assertion:
    // `drop` signals shutdown and joins the thread cleanly.
    let events = EventStream::new();
    drop(events);
}
