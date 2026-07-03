use std::{sync::mpsc, thread, time::Duration};

use termoxide_event::EventStream;

#[test]
fn check_receive() {
    let events = EventStream::new();
    events.recv().unwrap();
}

#[test]
fn handle_drop_stops_thread() {
    let events = EventStream::new();
    events.recv().unwrap();

    let (done_tx, done_rx) = mpsc::channel();
    thread::spawn(move || {
        drop(events);
        let _ = done_tx.send(());
    });

    done_rx.recv_timeout(Duration::from_secs(3)).expect(
        "EventHandle::drop did not complete: the thread was not stopped",
    );
}
