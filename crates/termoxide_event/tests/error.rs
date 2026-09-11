//! Integration tests for the public behaviour of [`Error`]: its messages, its
//! sources, and the bounds that let callers propagate it with `?`.
//!
//! The conversion from a thread panic payload is crate-private, so its tests
//! stay next to it in `src/error.rs`.

use std::{error::Error as _, io, sync::mpsc};

use termoxide_event::{Error, event::Event};

#[test]
fn display_channel_error_describes_dropped_receiver() {
    let error = Error::Channel(mpsc::SendError(Event::ChannelReady));
    assert_eq!(
        format!("{error}"),
        "event channel receiver was dropped: sending on a closed channel"
    );
}

#[test]
fn display_terminal_error_wraps_inner_message() {
    let inner = io::Error::other("boom");
    let error = Error::Terminal(inner);
    assert_eq!(format!("{error}"), "terminal operation failed: boom");
}

#[test]
fn display_reader_panic_with_and_without_message() {
    assert_eq!(
        format!("{}", Error::ReaderPanicked(Some("boom".to_owned()))),
        "terminal input reader thread panicked: boom"
    );
    assert_eq!(
        format!("{}", Error::ReaderPanicked(None)),
        "terminal input reader thread panicked"
    );
}

#[test]
fn source_exposes_the_inner_error() {
    let channel = Error::Channel(mpsc::SendError(Event::ChannelReady));
    assert!(
        channel
            .source()
            .and_then(|s| s.downcast_ref::<mpsc::SendError<Event>>())
            .is_some(),
        "Channel source should be the inner SendError"
    );

    let terminal = Error::Terminal(io::Error::other("boom"));
    assert!(
        terminal.source().and_then(|s| s.downcast_ref::<io::Error>()).is_some(),
        "Terminal source should be the inner io::Error"
    );
}

#[test]
fn source_is_empty_for_a_reader_panic() {
    assert!(Error::ReaderPanicked(Some("boom".to_owned())).source().is_none());
}

#[test]
fn error_converts_into_a_boxed_send_sync_error() {
    // `eyre::Report` and `anyhow::Error` only accept errors that are
    // `Error + Send + Sync + 'static`: this line stops compiling if a
    // variant ever breaks one of those bounds.
    let boxed: Box<dyn std::error::Error + Send + Sync + 'static> = Box::new(Error::ReaderPanicked(None));
    assert_eq!(boxed.to_string(), "terminal input reader thread panicked");
}
