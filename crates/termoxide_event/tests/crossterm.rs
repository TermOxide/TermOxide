use std::{sync::mpsc, thread, time::Duration};

use termoxide_event::crossterm::print_events;

#[test]
fn print_events_stops_when_shutdown_already_signaled() {
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    shutdown_tx.send(()).unwrap();

    let (done_tx, done_rx) = mpsc::channel();
    thread::spawn(move || {
        let result = print_events(&shutdown_rx);
        let _ = done_tx.send(result);
    });

    let result = done_rx.recv_timeout(Duration::from_secs(2)).expect(
        "print_events did not return: the shutdown did not break the loop",
    );
    result.expect("print_events returned an error");
}
