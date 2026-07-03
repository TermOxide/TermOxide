pub mod crossterm;
use std::{sync::mpsc, thread, time::Duration};

use crossterm::read_events;

pub struct EventStream {
    receiver: mpsc::Receiver<()>,
    shutdown: Option<mpsc::SyncSender<()>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl EventStream {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let (ready_tx, ready_rx) = mpsc::channel();
        let (shutdown_tx, shutdown_rx) = mpsc::sync_channel(1);

        let thread = thread::spawn(move || {
            if let Err(error) = ready_tx.send(()) {
                dbg!(error);
                return;
            }
            let _ = read_events(shutdown_rx);
        });

        Self {
            receiver: ready_rx,
            shutdown: Some(shutdown_tx),
            thread: Some(thread),
        }
    }

    pub fn recv(&self) -> Result<(), mpsc::RecvTimeoutError> {
        self.receiver.recv_timeout(Duration::from_secs(2))
    }

    pub fn teardown(mut self) -> thread::Result<()> { self.stop() }

    fn stop(&mut self) -> thread::Result<()> {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        match self.thread.take() {
            Some(thread) => thread.join(),
            None => Ok(()),
        }
    }
}

impl Drop for EventStream {
    fn drop(&mut self) { let _ = self.stop(); }
}
