use std::{sync::mpsc, thread, time::Duration};

use termoxide_event::EventStream;

#[test]
fn check_receive() {
    let events = EventStream::new();
    assert!(events.recv().is_ok(), "EventStream::recv() failed");
}

#[test]
fn handle_drop_stops_thread() {
    let events = EventStream::new();
    assert!(
        events.recv().is_ok(),
        "EventStream::recv() failed before dropping the handle"
    );

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
