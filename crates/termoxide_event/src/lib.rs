pub mod crossterm;
use std::{sync::mpsc, thread};
use crossterm::read_events;

pub struct EventHandle {
    shutdown: mpsc::SyncSender<()>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Drop for EventHandle {
    fn drop(&mut self) {
        let _ = self.shutdown.send(());
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

pub fn setup_events() -> (mpsc::Receiver<()>, EventHandle) {
    let (ready_tx, ready_rx) = mpsc::channel();
    let (shutdown_tx, shutdown_rx) = mpsc::sync_channel(1);

    let thread = thread::spawn(move || {
        if let Err(error) = ready_tx.send(()) {
            dbg!(error);
            return;
        }
        let _ = read_events(shutdown_rx);
    });

    let handle = EventHandle {
        shutdown: shutdown_tx,
        thread: Some(thread),
    };

    (ready_rx, handle)
}

#[cfg(test)]
mod tests {
    use std::{sync::mpsc, thread, time::Duration};

    use crate::setup_events;

    #[test]
    fn check_receive() {
        let (rx, _handle) = setup_events();
        rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(true);
    }

    #[test]
    fn handle_drop_stops_thread() {
        let (rx, handle) = setup_events();
        rx.recv().unwrap();

        let (done_tx, done_rx) = mpsc::channel();
        thread::spawn(move || {
            drop(handle);
            let _ = done_tx.send(());
        });

        done_rx
            .recv_timeout(Duration::from_secs(3))
            .expect("EventHandle::drop did not complete: the thread was not stopped");
    }
}
