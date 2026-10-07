use std::{
    marker::PhantomData,
    sync::mpsc::{self, RecvTimeoutError},
    thread,
    time::Duration,
};

use interprocess::local_socket::{RecvHalf, SendHalf, Stream, traits::Stream as _};
use serde::{Serialize, de::DeserializeOwned};

use super::{
    error::{IpcError, Result},
    frame::{read_frame, write_frame},
    message::{Request, Response},
};

/// One end of the connection, sending `S` and receiving `R`.
///
/// The two type parameters are swapped between the two ends, so neither side
/// can send a message meant for the other direction.
///
/// Dropping it does not close the connection: its reading thread keeps the
/// pipe open until the peer sends or closes. The connection closes for sure
/// when either process exits.
pub(crate) struct Channel<S, R> {
    writer: SendHalf,
    frames: mpsc::Receiver<Result<Vec<u8>>>,
    _messages: PhantomData<(S, R)>,
}

/// The host's end: sends requests, receives responses.
pub(crate) type HostChannel = Channel<Request, Response>;

/// The subprocess's end: sends responses, receives requests.
pub(crate) type ChildChannel = Channel<Response, Request>;

impl<S: Serialize, R: DeserializeOwned> Channel<S, R> {
    /// Takes over `stream`, reading it on a thread of its own so that
    /// [`recv`](Self::recv) can give up after a timeout: `interprocess` has no
    /// read timeout on Windows named pipes.
    pub(super) fn new(stream: Stream) -> Self {
        let (reader, writer) = stream.split();
        let (frames_tx, frames) = mpsc::channel();
        thread::spawn(move || read_frames(reader, &frames_tx));
        Self { writer, frames, _messages: PhantomData }
    }

    /// Sends `message` to the peer.
    ///
    /// Fails with `Closed` if the peer has gone away.
    pub(crate) fn send(&mut self, message: &S) -> Result<()> {
        let payload = postcard::to_allocvec(message).map_err(IpcError::Encode)?;
        write_frame(&mut self.writer, &payload)
    }

    /// Waits up to `timeout` for the next message from the peer.
    ///
    /// Fails with `Timeout` if nothing arrives in time, with `Closed` if the
    /// peer has gone away, and with `Decode` if the message is malformed.
    /// After a `Timeout` a late answer would be taken for the next one, so the
    /// channel must not be used again.
    pub(crate) fn recv(&mut self, timeout: Duration) -> Result<R> {
        match self.frames.recv_timeout(timeout) {
            Ok(frame) => postcard::from_bytes(&frame?).map_err(IpcError::Decode),
            Err(RecvTimeoutError::Timeout) => Err(IpcError::Timeout),
            Err(RecvTimeoutError::Disconnected) => Err(IpcError::Closed),
        }
    }
}

/// Forwards every frame read from `reader`, until a read fails or the channel
/// is dropped.
fn read_frames(mut reader: RecvHalf, frames: &mpsc::Sender<Result<Vec<u8>>>) {
    loop {
        let frame = read_frame(&mut reader);
        let failed = frame.is_err();
        if frames.send(frame).is_err() || failed {
            return;
        }
    }
}
