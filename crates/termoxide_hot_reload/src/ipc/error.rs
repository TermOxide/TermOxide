use std::{error::Error, fmt, io};

pub(crate) type Result<T> = std::result::Result<T, IpcError>;

/// Why an IPC operation failed.
///
/// `Timeout` and `Closed` are kept apart on purpose: a timeout means the peer
/// is still there but not answering, while `Closed` means it is gone.
#[derive(Debug)]
pub(crate) enum IpcError {
    /// The operation did not finish within the time it was given.
    Timeout,
    /// The peer closed its end of the pipe, normally because it exited.
    Closed,
    /// The operating system reported an I/O failure.
    Io(io::Error),
    /// A message could not be encoded.
    Encode(postcard::Error),
    /// A frame arrived intact but its payload is not a valid message.
    Decode(postcard::Error),
    /// A payload is longer than the allowed maximum, whether about to be sent
    /// or announced by a received frame.
    PayloadTooLarge { len: usize, max: usize },
}

impl fmt::Display for IpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IpcError::Timeout => write!(f, "IPC operation timed out"),
            IpcError::Closed => write!(f, "IPC peer closed the connection"),
            IpcError::Io(e) => write!(f, "I/O error: {e}"),
            IpcError::Encode(e) => write!(f, "could not encode IPC message: {e}"),
            IpcError::Decode(e) => write!(f, "malformed IPC message: {e}"),
            IpcError::PayloadTooLarge { len, max } => write!(f, "IPC payload too large: {len} > {max}"),
        }
    }
}

impl Error for IpcError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            IpcError::Io(e) => Some(e),
            IpcError::Encode(e) | IpcError::Decode(e) => Some(e),
            IpcError::Timeout | IpcError::Closed | IpcError::PayloadTooLarge { .. } => None,
        }
    }
}

impl From<io::Error> for IpcError {
    /// Errors that mean the peer is gone are reported as `Closed` rather than a
    /// generic I/O failure:
    /// - `BrokenPipe`: a write found the read end closed (peer exited).
    /// - `UnexpectedEof`: a read hit end-of-stream, at a frame boundary or mid-frame, because the peer closed its end.
    /// - `ConnectionReset` / `ConnectionAborted`: how a Unix socket reports the same.
    fn from(e: io::Error) -> Self {
        match e.kind() {
            io::ErrorKind::BrokenPipe
            | io::ErrorKind::UnexpectedEof
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted => IpcError::Closed,
            _ => IpcError::Io(e),
        }
    }
}
