//! # Terminal backend — the `crossterm` adapter
//!
//! This module is the only place in the crate that depends on a concrete
//! terminal library. It reads raw input from `crossterm`, translates it into
//! the backend-agnostic [`Event`] / [`KeyCode`] types from
//! [`crate::event`], and forwards it over a channel.
//!
//! Because all of the coupling lives here, supporting a different backend
//! (for example `termion`, or a native Windows console) is a matter of
//! providing another translation step and read loop — no consumer of the
//! crate needs to change.

use std::{fmt, io, sync::mpsc, time::Duration};

use crossterm::{
    event::{poll, read},
    terminal::{disable_raw_mode, enable_raw_mode},
};

use crate::event::{Event, KeyCode};

/// Error returned by the reader loop when it cannot make progress.
#[derive(Debug)]
pub enum SendEventError {
    /// The receiving end of the event channel was dropped, so translated
    /// events can no longer be delivered.
    ChannelError(mpsc::SendError<Event>),
    /// A `crossterm` terminal operation failed (polling, reading, or
    /// enabling/disabling raw mode).
    TerminalError(io::Error),
}

impl fmt::Display for SendEventError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SendEventError::ChannelError(error) => {
                write!(f, "event channel receiver was dropped: {error}")
            },
            SendEventError::TerminalError(error) => {
                write!(f, "terminal operation failed: {error}")
            },
        }
    }
}

impl std::error::Error for SendEventError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SendEventError::ChannelError(error) => Some(error),
            SendEventError::TerminalError(error) => Some(error),
        }
    }
}

/// Translate a raw `crossterm` event into a crate [`Event`].
///
/// Only key **press** events are kept (`KeyEventKind::Press`); key releases,
/// repeats, and every non-key event (mouse, resize, focus, paste) yield
/// `None` and are silently discarded. A recognised press further depends on
/// [`to_keycode`] succeeding, so a press on an unsupported key also yields
/// `None`.
fn translate(event: crossterm::event::Event) -> Option<Event> {
    match event {
        crossterm::event::Event::Key(key)
            if key.kind == crossterm::event::KeyEventKind::Press =>
        {
            to_keycode(key.code).map(Event::KeyPress)
        },
        _ => None,
    }
}

/// Map a `crossterm` key code onto the crate's [`KeyCode`].
///
/// Returns `None` for any key that has no corresponding [`KeyCode`] variant
/// (for example media or modifier keys), which lets [`translate`] drop it.
fn to_keycode(code: crossterm::event::KeyCode) -> Option<KeyCode> {
    use crossterm::event::KeyCode as Ct;
    match code {
        Ct::Backspace => Some(KeyCode::Backspace),
        Ct::Enter => Some(KeyCode::Enter),
        Ct::Left => Some(KeyCode::Left),
        Ct::Right => Some(KeyCode::Right),
        Ct::Up => Some(KeyCode::Up),
        Ct::Down => Some(KeyCode::Down),
        Ct::Home => Some(KeyCode::Home),
        Ct::End => Some(KeyCode::End),
        Ct::PageUp => Some(KeyCode::PageUp),
        Ct::PageDown => Some(KeyCode::PageDown),
        Ct::Tab => Some(KeyCode::Tab),
        Ct::BackTab => Some(KeyCode::BackTab),
        Ct::Delete => Some(KeyCode::Delete),
        Ct::Insert => Some(KeyCode::Insert),
        Ct::F(n) => Some(KeyCode::F(n)),
        Ct::Char(c) => Some(KeyCode::Char(c)),
        Ct::Null => Some(KeyCode::Null),
        Ct::Esc => Some(KeyCode::Esc),
        _ => None,
    }
}

/// Run the input loop, forwarding translated events until asked to stop.
///
/// Each iteration first checks `shutdown`: the loop returns `Ok(())` as soon
/// as a stop signal is received *or* the sender side is disconnected.
/// Otherwise it polls the terminal for up to 100 ms, and on activity reads one
/// event, translates it, and sends the result over `events_tx`.
///
/// The 100 ms poll timeout bounds how long a shutdown request can take to be
/// noticed: the loop reacts within at most one poll interval.
///
/// This function does **not** manage raw mode; it expects the terminal to be
/// prepared by the caller (see [`read_events`]).
///
/// # Errors
///
/// - [`SendEventError::TerminalError`] if polling or reading fails.
/// - [`SendEventError::ChannelError`] if the receiver has been dropped and a
///   translated event can no longer be delivered.
fn send_events(
    events_tx: &mpsc::Sender<Event>,
    shutdown: &mpsc::Receiver<()>,
) -> Result<(), SendEventError> {
    loop {
        match shutdown.try_recv() {
            Ok(()) | Err(mpsc::TryRecvError::Disconnected) => break,
            Err(mpsc::TryRecvError::Empty) => {},
        }

        if poll(Duration::from_millis(100))
            .map_err(SendEventError::TerminalError)?
        {
            let event = read().map_err(SendEventError::TerminalError)?;
            if let Some(translated_event) = translate(event) {
                events_tx
                    .send(translated_event)
                    .map_err(SendEventError::ChannelError)?;
            }
        }
    }

    Ok(())
}

/// Enable raw mode, run [`send_events`], and restore the terminal afterwards.
///
/// This is the entry point run on the background reader thread. It brackets
/// the loop with `enable_raw_mode` / `disable_raw_mode` so the terminal is
/// always left in a sane state, even when the loop stops because of an error.
///
/// # Errors
///
/// Returns the error produced by [`send_events`], if any. When the loop itself
/// succeeded but `disable_raw_mode` fails, that teardown failure is surfaced
/// instead as a [`SendEventError::TerminalError`]; a loop error takes
/// precedence over a teardown error and is preserved unchanged.
pub(crate) fn read_events(
    events_tx: mpsc::Sender<Event>,
    shutdown_rx: mpsc::Receiver<()>,
) -> Result<(), SendEventError> {
    enable_raw_mode().map_err(SendEventError::TerminalError)?;

    let result = send_events(&events_tx, &shutdown_rx);

    if let Err(disable_error) = disable_raw_mode()
        && result.is_ok()
    {
        return Err(SendEventError::TerminalError(disable_error));
    }

    result
}

#[cfg(test)]
mod tests {
    use std::thread;

    use crossterm::event::{
        KeyCode as Ct,
        KeyEvent,
        KeyEventKind,
        KeyModifiers,
        MediaKeyCode,
        ModifierKeyCode,
        MouseEvent,
        MouseEventKind,
    };

    use super::*;

    #[test]
    fn send_events_stops_when_shutdown_already_signaled() {
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let (events_tx, _events_rx) = mpsc::channel();
        // Signal shutdown up front, so the loop must exit on its very first
        // iteration without ever blocking on a terminal poll.
        assert!(
            shutdown_tx.send(()).is_ok(),
            "Failed to send shutdown signal"
        );

        // Run the loop on a side thread and wait with a timeout: if it fails to
        // honour the pre-set shutdown signal and blocks, the test fails after
        // 2s instead of hanging forever.
        let (done_tx, done_rx) = mpsc::channel();
        thread::spawn(move || {
            let result = send_events(&events_tx, &shutdown_rx);
            let _ = done_tx.send(result);
        });

        let result = done_rx.recv_timeout(Duration::from_secs(2));
        assert!(
            result.is_ok(),
            "send_events returned an error: {:?}",
            result.err()
        );
    }

    /// Wrap a `crossterm` key code and kind into a full terminal event.
    ///
    /// Keeps the `translate` tests focused on the two axes that matter here —
    /// the key code and the event kind — without repeating the modifier and
    /// wrapper boilerplate at every call site.
    fn key_event(code: Ct, kind: KeyEventKind) -> crossterm::event::Event {
        crossterm::event::Event::Key(KeyEvent::new_with_kind(
            code,
            KeyModifiers::NONE,
            kind,
        ))
    }

    #[test]
    fn to_keycode_maps_every_supported_key() {
        let cases = [
            (Ct::Backspace, KeyCode::Backspace),
            (Ct::Enter, KeyCode::Enter),
            (Ct::Left, KeyCode::Left),
            (Ct::Right, KeyCode::Right),
            (Ct::Up, KeyCode::Up),
            (Ct::Down, KeyCode::Down),
            (Ct::Home, KeyCode::Home),
            (Ct::End, KeyCode::End),
            (Ct::PageUp, KeyCode::PageUp),
            (Ct::PageDown, KeyCode::PageDown),
            (Ct::Tab, KeyCode::Tab),
            (Ct::BackTab, KeyCode::BackTab),
            (Ct::Delete, KeyCode::Delete),
            (Ct::Insert, KeyCode::Insert),
            (Ct::Null, KeyCode::Null),
            (Ct::Esc, KeyCode::Esc),
        ];

        for (input, expected) in cases {
            assert_eq!(
                to_keycode(input),
                Some(expected),
                "unexpected mapping for {input:?}"
            );
        }
    }

    #[test]
    fn to_keycode_preserves_function_key_number() {
        for n in [0u8, 1, 5, 12, 255] {
            assert_eq!(to_keycode(Ct::F(n)), Some(KeyCode::F(n)));
        }
    }

    #[test]
    fn to_keycode_preserves_char_value() {
        for c in ['a', 'Z', '9', ' ', 'é', '\n', '\0'] {
            assert_eq!(to_keycode(Ct::Char(c)), Some(KeyCode::Char(c)));
        }
    }

    #[test]
    fn to_keycode_returns_none_for_unsupported_keys() {
        let unsupported = [
            Ct::CapsLock,
            Ct::ScrollLock,
            Ct::NumLock,
            Ct::PrintScreen,
            Ct::Pause,
            Ct::Menu,
            Ct::KeypadBegin,
            Ct::Media(MediaKeyCode::Play),
            Ct::Modifier(ModifierKeyCode::LeftShift),
        ];

        for input in unsupported {
            assert_eq!(to_keycode(input), None, "expected None for {input:?}");
        }
    }

    #[test]
    fn translate_keeps_supported_key_press() {
        let event = key_event(Ct::Char('x'), KeyEventKind::Press);
        assert_eq!(translate(event), Some(Event::KeyPress(KeyCode::Char('x'))));
    }

    #[test]
    fn translate_drops_press_on_unsupported_key() {
        let event = key_event(Ct::CapsLock, KeyEventKind::Press);
        assert_eq!(translate(event), None);
    }

    #[test]
    fn translate_drops_key_release_and_repeat() {
        for kind in [KeyEventKind::Release, KeyEventKind::Repeat] {
            let event = key_event(Ct::Char('x'), kind);
            assert_eq!(translate(event), None, "expected None for {kind:?}");
        }
    }

    #[test]
    fn translate_drops_non_key_events() {
        let events = [
            crossterm::event::Event::Resize(80, 24),
            crossterm::event::Event::FocusGained,
            crossterm::event::Event::FocusLost,
            crossterm::event::Event::Paste("hello".to_string()),
            crossterm::event::Event::Mouse(MouseEvent {
                kind: MouseEventKind::Moved,
                column: 0,
                row: 0,
                modifiers: KeyModifiers::NONE,
            }),
        ];

        for event in events {
            let described = format!("{event:?}");
            assert_eq!(translate(event), None, "expected None for {described}");
        }
    }

    #[test]
    fn display_channel_error_describes_dropped_receiver() {
        let error =
            SendEventError::ChannelError(mpsc::SendError(Event::ChannelReady));
        assert_eq!(
            format!("{error}"),
            "event channel receiver was dropped: sending on a closed channel"
        );
    }

    #[test]
    fn display_terminal_error_wraps_inner_message() {
        let inner = io::Error::other("boom");
        let error = SendEventError::TerminalError(inner);
        assert_eq!(format!("{error}"), "terminal operation failed: boom");
    }

    #[test]
    fn source_exposes_the_inner_error() {
        use std::error::Error as _;

        let channel =
            SendEventError::ChannelError(mpsc::SendError(Event::ChannelReady));
        assert!(
            channel
                .source()
                .and_then(|s| s.downcast_ref::<mpsc::SendError<Event>>())
                .is_some(),
            "ChannelError source should be the inner SendError"
        );

        let terminal = SendEventError::TerminalError(io::Error::other("boom"));
        assert!(
            terminal
                .source()
                .and_then(|s| s.downcast_ref::<io::Error>())
                .is_some(),
            "TerminalError source should be the inner io::Error"
        );
    }
}
