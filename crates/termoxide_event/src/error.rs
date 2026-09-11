//! # Errors — the single error type of the crate
//!
//! Every fallible operation of `termoxide_event` reports an [`Error`], and
//! [`Result`] is the matching alias, following the `std::io::Error` /
//! `std::io::Result` pattern. Having one flat error type means a caller can
//! propagate any failure of the crate with `?`, without unwrapping nested
//! results or converting between several error types.

use std::{any::Any, fmt, io, sync::mpsc};

use crate::event::Event;

/// Error reported by the terminal-input reader.
#[derive(Debug)]
pub enum Error {
    /// The receiving end of the event channel was dropped, so translated
    /// events can no longer be delivered.
    Channel(mpsc::SendError<Event>),
    /// A `crossterm` terminal operation failed (polling, reading, or
    /// enabling/disabling raw mode).
    Terminal(io::Error),
    /// The reader thread panicked. Carries the panic message when the payload
    /// was a string, which is the case for every `panic!` with a message.
    ReaderPanicked(Option<String>),
}

/// A specialized [`Result`](std::result::Result) whose error defaults to the
/// crate's [`Error`].
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    /// Build an [`Error::ReaderPanicked`] from the payload of a joined thread.
    ///
    /// The raw payload (`Box<dyn Any + Send>`) is neither an error nor `Sync`,
    /// so it cannot be stored as is without making [`Error`] unusable with
    /// `?` in `eyre` / `anyhow`. Only its message is kept: `panic!` yields a
    /// `&'static str` for a literal message and a `String` for a formatted one;
    /// any other payload has no message to keep.
    pub(crate) fn from_panic(payload: Box<dyn Any + Send>) -> Self {
        let message = match payload.downcast::<String>() {
            Ok(message) => Some(*message),
            Err(payload) => payload.downcast_ref::<&str>().map(|message| (*message).to_owned()),
        };
        Error::ReaderPanicked(message)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Channel(error) => {
                write!(f, "event channel receiver was dropped: {error}")
            },
            Error::Terminal(error) => {
                write!(f, "terminal operation failed: {error}")
            },
            Error::ReaderPanicked(Some(message)) => {
                write!(f, "terminal input reader thread panicked: {message}")
            },
            Error::ReaderPanicked(None) => write!(f, "terminal input reader thread panicked"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Channel(error) => Some(error),
            Error::Terminal(error) => Some(error),
            Error::ReaderPanicked(_) => None,
        }
    }
}

// Only the tests of the crate-private `from_panic` live here; the public
// behaviour of `Error` is covered by `tests/error.rs`.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_panic_keeps_a_literal_message() {
        let error = Error::from_panic(Box::new("boom"));
        assert!(matches!(error, Error::ReaderPanicked(Some(ref message)) if message == "boom"));
    }

    #[test]
    fn from_panic_keeps_a_formatted_message() {
        let error = Error::from_panic(Box::new(format!("boom {}", 42)));
        assert!(matches!(error, Error::ReaderPanicked(Some(ref message)) if message == "boom 42"));
    }

    #[test]
    fn from_panic_drops_a_non_string_payload() {
        let error = Error::from_panic(Box::new(42));
        assert!(matches!(error, Error::ReaderPanicked(None)));
    }

    #[test]
    #[allow(clippy::panic)]
    fn from_panic_reads_the_payload_of_a_real_panic() {
        // Same shape as the reader thread: a closure returning `Result<()>`.
        let joined = std::thread::spawn(|| -> Result<()> { panic!("reader exploded") }).join();

        let error = joined.map_err(Error::from_panic);
        assert!(matches!(error, Err(Error::ReaderPanicked(Some(ref message))) if message == "reader exploded"));
    }
}
