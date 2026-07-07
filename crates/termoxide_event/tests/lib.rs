//! Integration tests for the [`EventStream`] lifecycle: receiving events and
//! shutting the reader thread down, both via an explicit `teardown` and via
//! `Drop`.
//!
//! Every test is marked `#[serial]` because each one puts the real terminal
//! into raw mode; running two at once would let them fight over the terminal
//! and interleave raw-mode toggling non-deterministically.

use std::{sync::mpsc, thread, time::Duration};

use serial_test::serial;
use termoxide_event::EventStream;

#[test]
#[serial]
fn check_receive() {
    let events = EventStream::new();
    assert!(events.recv().is_ok(), "EventStream::recv() failed");
}

#[test]
#[serial]
fn handle_drop_stops_thread() {
    let events = EventStream::new();
    assert!(
        events.recv().is_ok(),
        "EventStream::recv() failed before dropping the handle"
    );

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
    assert!(
        events.recv().is_ok(),
        "EventStream::recv() failed before calling teardown"
    );

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
    // Drop the stream right away, without ever calling `recv`. Depending on
    // timing the reader thread may or may not have sent `ChannelReady` yet, so
    // the `ChannelReady` send may succeed or fail — either way the thread must
    // shut down without panicking. Reaching the end of the test is the
    // assertion: `drop` signals shutdown and joins the thread cleanly.
    let events = EventStream::new();
    drop(events);
}
