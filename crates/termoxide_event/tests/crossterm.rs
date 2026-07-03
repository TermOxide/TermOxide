use std::{sync::mpsc, thread, time::Duration};

use termoxide_event::crossterm::print_events;

#[test]
fn print_events_stops_when_shutdown_already_signaled() {
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    assert!(
        shutdown_tx.send(()).is_ok(),
        "Failed to send shutdown signal"
    );

    let (done_tx, done_rx) = mpsc::channel();
    thread::spawn(move || {
        let result = print_events(&shutdown_rx);
        let _ = done_tx.send(result);
    });

    let result = done_rx.recv_timeout(Duration::from_secs(2));
    assert!(
        result.is_ok(),
        "print_events returned an error: {:?}",
        result.err()
    );
}
