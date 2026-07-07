//! # termoxide_event: Terminal input events for TermOxide
//!
//! This crate turns raw terminal input into typed, backend-agnostic
//! [`Event`]s delivered asynchronously over a channel. A background thread
//! reads from the terminal (through `crossterm`), translates each input, and
//! forwards it to the application, which pulls events at its own pace.
//!
//! ## Overview
//!
//! |Type|Role|
//! |------|------|
//! |[`EventStream`]|Handle owning the reader thread|
//! |[`Event`]|A single event (handshake or key press) delivered by the stream|
//! |[`KeyCode`](event::KeyCode)|Backend-agnostic key identifier|
//! |[`SendEventError`](backend::SendEventError)|Error from the reader loop|
//!
//! Raw mode is enabled while the stream is alive and restored when it is
//! dropped, so the terminal is never left in a broken state.
//!
//! ## Quickstart
//!
//! ```no_run
//! use termoxide_event::{EventStream, event::Event};
//!
//! let events = EventStream::new();
//!
//! // The first event is always `Event::ChannelReady`: the reader is live.
//! while let Ok(event) = events.recv() {
//!     match event {
//!         Event::ChannelReady => println!("stream ready"),
//!         Event::KeyPress(code) => println!("key pressed: {code:?}"),
//!     }
//! }
//!
//! // Stop the reader thread and restore the terminal.
//! events.teardown().expect("reader thread panicked");
//! ```

pub mod backend;
pub mod event;
use std::{sync::mpsc, thread};

use backend::read_events;
use event::Event;

/// An owning handle over a background terminal-input reader.
///
/// Creating an `EventStream` spawns a thread that puts the terminal into raw
/// mode and streams [`Event`]s back over a channel. The application consumes
/// them with [`recv`](Self::recv). The very first event is always
/// [`Event::ChannelReady`].
///
/// Shutdown is cooperative: dropping the handle (or calling
/// [`teardown`](Self::teardown)) signals the thread to stop, joins it, and
/// lets it restore the terminal. Thanks to the [`Drop`] implementation this
/// happens even if the handle simply goes out of scope.
pub struct EventStream {
    /// Receiving end of the channel translated events arrive on.
    receiver: mpsc::Receiver<Event>,
    /// Sender used to signal the reader thread to stop; taken on shutdown so
    /// stopping is idempotent.
    shutdown: Option<mpsc::SyncSender<()>>,
    /// Join handle of the reader thread; taken on shutdown so it is joined at
    /// most once.
    thread: Option<thread::JoinHandle<()>>,
}

impl EventStream {
    /// Create a stream and start reading terminal input.
    ///
    /// Spawns the reader thread, which immediately emits
    /// [`Event::ChannelReady`] and then runs the internal read loop,
    /// enabling raw mode for the lifetime of the stream. Returns as soon as
    /// the thread is spawned, without blocking on the first event.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let (events_tx, events_rx) = mpsc::channel();
        let (shutdown_tx, shutdown_rx) = mpsc::sync_channel(1);

        let thread = thread::spawn(move || {
            if let Err(error) = events_tx.send(Event::ChannelReady) {
                dbg!(error);
                return;
            }
            if let Err(error) = read_events(events_tx, shutdown_rx) {
                dbg!(error);
            }
        });

        Self {
            receiver: events_rx,
            shutdown: Some(shutdown_tx),
            thread: Some(thread),
        }
    }

    /// Block until the next event arrives.
    ///
    /// Waits indefinitely for the reader thread to produce an event. The very
    /// first call is guaranteed to return [`Event::ChannelReady`], so it never
    /// hangs on a freshly created stream. Returns
    /// [`RecvError`](mpsc::RecvError) once the reader thread has stopped and
    /// the channel is closed.
    ///
    /// This is a temporary interface that will be superseded by a dedicated
    /// polling method.
    pub fn recv(&self) -> Result<Event, mpsc::RecvError> {
        self.receiver.recv()
    }

    /// Stop the stream explicitly and wait for the reader thread to finish.
    ///
    /// Consumes the handle, signals shutdown, and joins the thread — the same
    /// work the [`Drop`] implementation performs, except the join result is
    /// returned rather than ignored. The [`thread::Result`] is `Err` only if
    /// the reader thread panicked.
    pub fn teardown(mut self) -> thread::Result<()> { self.stop() }

    /// Signal the reader thread to stop and join it, at most once.
    ///
    /// Both the shutdown sender and the join handle are taken out of their
    /// `Option` slots, so repeated calls (for instance
    /// [`teardown`](Self::teardown) followed by [`Drop`]) are safe no-ops.
    /// Sending the shutdown signal is best-effort: if the thread has
    /// already exited the send simply fails and is ignored.
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
    /// Stop the reader thread when the handle goes out of scope.
    ///
    /// This is the RAII guarantee that raw mode is disabled and the terminal
    /// restored even if the caller never calls
    /// [`teardown`](EventStream::teardown). The join result is discarded
    /// here; use `teardown` to observe it.
    fn drop(&mut self) { let _ = self.stop(); }
}
