pub mod backend;
pub mod event;
use std::{sync::mpsc, thread, time::Duration};

use backend::read_events;
use event::Event;

pub struct EventStream {
    receiver: mpsc::Receiver<Event>,
    shutdown: Option<mpsc::SyncSender<()>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl EventStream {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let (events_tx, events_rx) = mpsc::channel();
        let (shutdown_tx, shutdown_rx) = mpsc::sync_channel(1);

        let thread = thread::spawn(move || {
            if let Err(error) = events_tx.send(Event::ChannelReady) {
                dbg!(error);
                return;
            }
            if let Err(error) = read_events(shutdown_rx) {
                dbg!(error);
            }
        });

        Self {
            receiver: events_rx,
            shutdown: Some(shutdown_tx),
            thread: Some(thread),
        }
    }

    pub fn recv(&self) -> Result<Event, mpsc::RecvTimeoutError> {
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
