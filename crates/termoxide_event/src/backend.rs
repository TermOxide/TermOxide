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

use std::{io::stdout, sync::mpsc, time::Duration};

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, poll, read},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode},
};

use crate::{
    config::EventStreamConfig,
    error::{Error, Result},
    event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind},
};

/// Translate a raw `crossterm` event into a crate [`Event`].
///
/// Key **presses** (`KeyEventKind::Press`) and mouse actions are kept; key
/// releases, repeats, and every remaining event (resize, focus, paste) yield
/// `None` and are silently discarded. A recognised press further depends on
/// [`to_keycode`] succeeding, so a press on an unsupported key also yields
/// `None`. A mouse action, on the other hand, always translates:
/// [`to_mouse_kind`] covers every kind `crossterm` reports.
fn translate(event: crossterm::event::Event) -> Option<Event> {
    match event {
        crossterm::event::Event::Key(key) if key.kind == crossterm::event::KeyEventKind::Press => {
            let code = to_keycode(key.code)?;
            Some(Event::KeyPress(KeyEvent::new(code, to_modifiers(key.modifiers))))
        },
        crossterm::event::Event::Mouse(mouse) => {
            Some(Event::Mouse(MouseEvent::new(
                to_mouse_kind(mouse.kind),
                mouse.column,
                mouse.row,
                to_modifiers(mouse.modifiers),
            )))
        },
        _ => None,
    }
}

/// Whether a translated `event` may be delivered under `config`.
///
/// Opting out of mouse capture only skips turning reporting on: the terminal
/// may still be reporting the mouse, for example after an application that
/// crashed without restoring it. Mouse events are dropped here so an opted-out
/// stream never delivers one.
fn accepts(event: &Event, config: EventStreamConfig) -> bool {
    config.mouse_capture || !matches!(event, Event::Mouse(_))
}

/// Map `crossterm` modifier flags onto the crate's [`KeyModifiers`].
///
/// Unlike [`to_keycode`], an unrecognised flag cannot fail the whole
/// translation: modifiers combine, so any flag without a counterpart here is
/// simply left out of the resulting set.
fn to_modifiers(modifiers: crossterm::event::KeyModifiers) -> KeyModifiers {
    use crossterm::event::KeyModifiers as Ct;

    let mut translated = KeyModifiers::NONE;
    for (raw, mapped) in [
        (Ct::SHIFT, KeyModifiers::SHIFT),
        (Ct::CONTROL, KeyModifiers::CONTROL),
        (Ct::ALT, KeyModifiers::ALT),
        (Ct::SUPER, KeyModifiers::SUPER),
    ] {
        if modifiers.contains(raw) {
            translated |= mapped;
        }
    }
    translated
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

/// Map a `crossterm` mouse button onto the crate's [`MouseButton`].
///
/// Total, unlike [`to_keycode`]: a terminal reports exactly these three
/// buttons.
fn to_button(button: crossterm::event::MouseButton) -> MouseButton {
    use crossterm::event::MouseButton as Ct;
    match button {
        Ct::Left => MouseButton::Left,
        Ct::Right => MouseButton::Right,
        Ct::Middle => MouseButton::Middle,
    }
}

/// Map a `crossterm` mouse event kind onto the crate's [`MouseEventKind`].
///
/// Total as well: every kind `crossterm` can report has a counterpart here, so
/// no mouse action is ever dropped on the way through.
fn to_mouse_kind(kind: crossterm::event::MouseEventKind) -> MouseEventKind {
    use crossterm::event::MouseEventKind as Ct;
    match kind {
        Ct::Down(button) => MouseEventKind::Down(to_button(button)),
        Ct::Up(button) => MouseEventKind::Up(to_button(button)),
        Ct::Drag(button) => MouseEventKind::Drag(to_button(button)),
        Ct::Moved => MouseEventKind::Moved,
        Ct::ScrollUp => MouseEventKind::ScrollUp,
        Ct::ScrollDown => MouseEventKind::ScrollDown,
        Ct::ScrollLeft => MouseEventKind::ScrollLeft,
        Ct::ScrollRight => MouseEventKind::ScrollRight,
    }
}

/// Run the input loop, forwarding translated events until asked to stop.
///
/// Each iteration first checks `shutdown`: the loop returns `Ok(())` as soon
/// as a stop signal is received *or* the sender side is disconnected.
/// Otherwise it polls the terminal for up to 100 ms, and on activity reads one
/// event, translates it, and sends the result over `events_tx` — unless
/// `config` rejects it (see [`accepts`]).
///
/// The 100 ms poll timeout bounds how long a shutdown request can take to be
/// noticed: the loop reacts within at most one poll interval.
///
/// This function does **not** manage raw mode; it expects the terminal to be
/// prepared by the caller (see [`read_events`]).
///
/// # Errors
///
/// - [`Error::Terminal`] if polling or reading fails.
/// - [`Error::Channel`] if the receiver has been dropped and a translated event can no longer be delivered.
fn send_events(
    events_tx: &mpsc::Sender<Event>,
    shutdown: &mpsc::Receiver<()>,
    config: EventStreamConfig,
) -> Result<()> {
    loop {
        match shutdown.try_recv() {
            Ok(()) | Err(mpsc::TryRecvError::Disconnected) => break,
            Err(mpsc::TryRecvError::Empty) => {},
        }

        if poll(Duration::from_millis(100)).map_err(Error::Terminal)? {
            let event = read().map_err(Error::Terminal)?;
            if let Some(translated_event) = translate(event)
                && accepts(&translated_event, config)
            {
                events_tx.send(translated_event).map_err(Error::Channel)?;
            }
        }
    }

    Ok(())
}

/// Put the terminal into the modes the reader needs: raw input and, unless
/// `config` opts out of it, mouse reporting.
///
/// Mouse capture is on by default, so any application driven by the stream can
/// react to clicks without asking for them. While it is on, the terminal stops
/// handling text selection itself — most terminals still select with `Shift`
/// held — which is why an application that ignores the mouse can turn it off
/// (ADR-0007).
///
/// Enabling capture after raw mode is deliberate, and so is undoing raw mode
/// when it fails: the caller has no handle to restore the terminal with, so a
/// half-finished setup would leave the terminal raw for good.
///
/// # Errors
///
/// - [`Error::Terminal`] if raw mode or mouse reporting cannot be enabled.
fn setup_terminal(config: EventStreamConfig) -> Result<()> {
    enable_raw_mode().map_err(Error::Terminal)?;

    if !config.mouse_capture {
        return Ok(());
    }

    if let Err(error) = execute!(stdout(), EnableMouseCapture) {
        let _ = disable_raw_mode();
        return Err(Error::Terminal(error));
    }

    Ok(())
}

/// Undo [`setup_terminal`], in reverse order.
///
/// Both restorations are attempted even when the first one fails, so a broken
/// step never leaves the other mode enabled behind it; the first failure is
/// the one reported. Mouse reporting is left alone when `config` opted out of
/// it: the setup never turned it on, and disabling it anyway would write
/// escape sequences to a possibly redirected stdout, or switch off reporting
/// that another program owns.
///
/// # Errors
///
/// - [`Error::Terminal`] if mouse reporting or raw mode cannot be disabled.
fn restore_terminal(config: EventStreamConfig) -> Result<()> {
    let mouse = if config.mouse_capture {
        execute!(stdout(), DisableMouseCapture)
    } else {
        Ok(())
    };
    let raw = disable_raw_mode();

    mouse.map_err(Error::Terminal)?;
    raw.map_err(Error::Terminal)
}

/// Prepare the terminal, run [`send_events`], and restore the terminal
/// afterwards.
///
/// This is the entry point run on the background reader thread. It brackets
/// the loop with [`setup_terminal`] / [`restore_terminal`] so the terminal is
/// always left in a sane state, even when the loop stops because of an error.
///
/// # Errors
///
/// Returns the error produced by [`send_events`], if any. When the loop itself
/// succeeded but restoring the terminal fails, that teardown failure is
/// surfaced instead as an [`Error::Terminal`]; a loop error takes precedence
/// over a teardown error and is preserved unchanged.
pub(crate) fn read_events(
    events_tx: mpsc::Sender<Event>,
    shutdown_rx: mpsc::Receiver<()>,
    config: EventStreamConfig,
) -> Result<()> {
    setup_terminal(config)?;

    let result = send_events(&events_tx, &shutdown_rx, config);

    if let Err(restore_error) = restore_terminal(config)
        && result.is_ok()
    {
        return Err(restore_error);
    }

    result
}

#[cfg(test)]
mod tests {
    use std::thread;

    // `crossterm` types are aliased so they never shadow the crate's own
    // `KeyEvent` / `KeyModifiers` pulled in by `use super::*`.
    use crossterm::event::{
        KeyCode as Ct,
        KeyEvent as CtKeyEvent,
        KeyEventKind,
        KeyModifiers as CtModifiers,
        MediaKeyCode,
        ModifierKeyCode,
        MouseButton as CtButton,
        MouseEvent as CtMouseEvent,
        MouseEventKind as CtMouseKind,
    };

    use super::*;

    #[test]
    fn send_events_stops_when_shutdown_already_signaled() {
        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let (events_tx, _events_rx) = mpsc::channel();
        // Signal shutdown up front, so the loop must exit on its very first
        // iteration without ever blocking on a terminal poll.
        assert!(shutdown_tx.send(()).is_ok(), "Failed to send shutdown signal");

        // Run the loop on a side thread and wait with a timeout: if it fails to
        // honour the pre-set shutdown signal and blocks, the test fails after
        // 2s instead of hanging forever.
        let (done_tx, done_rx) = mpsc::channel();
        thread::spawn(move || {
            let result = send_events(&events_tx, &shutdown_rx, EventStreamConfig::default());
            let _ = done_tx.send(result);
        });

        let result = done_rx.recv_timeout(Duration::from_secs(2));
        assert!(result.is_ok(), "send_events returned an error: {:?}", result.err());
    }

    /// Wrap a `crossterm` key code and kind into a full terminal event.
    ///
    /// Keeps the `translate` tests focused on the two axes that matter here —
    /// the key code and the event kind — without repeating the modifier and
    /// wrapper boilerplate at every call site.
    fn key_event(code: Ct, kind: KeyEventKind) -> crossterm::event::Event {
        key_event_with(code, CtModifiers::NONE, kind)
    }

    /// Same as [`key_event`], but with explicit modifiers.
    fn key_event_with(code: Ct, modifiers: CtModifiers, kind: KeyEventKind) -> crossterm::event::Event {
        crossterm::event::Event::Key(CtKeyEvent::new_with_kind(code, modifiers, kind))
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
            assert_eq!(to_keycode(input), Some(expected), "unexpected mapping for {input:?}");
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
    fn to_modifiers_maps_every_supported_modifier() {
        let cases = [
            (CtModifiers::NONE, KeyModifiers::NONE),
            (CtModifiers::SHIFT, KeyModifiers::SHIFT),
            (CtModifiers::CONTROL, KeyModifiers::CONTROL),
            (CtModifiers::ALT, KeyModifiers::ALT),
            (CtModifiers::SUPER, KeyModifiers::SUPER),
        ];

        for (input, expected) in cases {
            assert_eq!(to_modifiers(input), expected, "unexpected mapping for {input:?}");
        }
    }

    #[test]
    fn to_modifiers_preserves_combinations() {
        let translated = to_modifiers(CtModifiers::CONTROL | CtModifiers::SHIFT | CtModifiers::ALT);

        assert_eq!(translated, KeyModifiers::CONTROL | KeyModifiers::SHIFT | KeyModifiers::ALT);
        assert!(!translated.contains(KeyModifiers::SUPER));
    }

    #[test]
    fn to_modifiers_drops_unsupported_flags() {
        // HYPER and META have no counterpart, so they must vanish without
        // dragging the recognised flags down with them.
        let translated = to_modifiers(CtModifiers::HYPER | CtModifiers::META | CtModifiers::CONTROL);

        assert_eq!(translated, KeyModifiers::CONTROL);
    }

    #[test]
    fn translate_keeps_supported_key_press() {
        let event = key_event(Ct::Char('x'), KeyEventKind::Press);
        assert_eq!(
            translate(event),
            Some(Event::KeyPress(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE)))
        );
    }

    #[test]
    fn translate_carries_modifiers_alongside_the_key() {
        let event = key_event_with(Ct::Char('c'), CtModifiers::CONTROL, KeyEventKind::Press);

        assert_eq!(
            translate(event),
            Some(Event::KeyPress(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL))),
            "Ctrl+C must stay distinguishable from a plain 'c'"
        );
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
    fn translate_drops_events_carrying_no_input() {
        let events = [
            crossterm::event::Event::Resize(80, 24),
            crossterm::event::Event::FocusGained,
            crossterm::event::Event::FocusLost,
            crossterm::event::Event::Paste("hello".to_string()),
        ];

        for event in events {
            let described = format!("{event:?}");
            assert_eq!(translate(event), None, "expected None for {described}");
        }
    }

    /// Wrap a `crossterm` mouse kind and position into a full terminal event.
    ///
    /// Mirrors [`key_event`] for the mouse side of [`translate`].
    fn mouse_event(kind: CtMouseKind, column: u16, row: u16, modifiers: CtModifiers) -> crossterm::event::Event {
        crossterm::event::Event::Mouse(CtMouseEvent { kind, column, row, modifiers })
    }

    #[test]
    fn to_button_maps_every_button() {
        let cases = [
            (CtButton::Left, MouseButton::Left),
            (CtButton::Right, MouseButton::Right),
            (CtButton::Middle, MouseButton::Middle),
        ];

        for (input, expected) in cases {
            assert_eq!(to_button(input), expected, "unexpected mapping for {input:?}");
        }
    }

    #[test]
    fn to_mouse_kind_maps_every_kind() {
        let cases = [
            (CtMouseKind::Down(CtButton::Left), MouseEventKind::Down(MouseButton::Left)),
            (CtMouseKind::Up(CtButton::Right), MouseEventKind::Up(MouseButton::Right)),
            (CtMouseKind::Drag(CtButton::Middle), MouseEventKind::Drag(MouseButton::Middle)),
            (CtMouseKind::Moved, MouseEventKind::Moved),
            (CtMouseKind::ScrollUp, MouseEventKind::ScrollUp),
            (CtMouseKind::ScrollDown, MouseEventKind::ScrollDown),
            (CtMouseKind::ScrollLeft, MouseEventKind::ScrollLeft),
            (CtMouseKind::ScrollRight, MouseEventKind::ScrollRight),
        ];

        for (input, expected) in cases {
            assert_eq!(to_mouse_kind(input), expected, "unexpected mapping for {input:?}");
        }
    }

    #[test]
    fn translate_keeps_a_mouse_action_with_its_position() {
        let event = mouse_event(CtMouseKind::Down(CtButton::Left), 12, 34, CtModifiers::NONE);

        assert_eq!(
            translate(event),
            Some(Event::Mouse(MouseEvent::new(
                MouseEventKind::Down(MouseButton::Left),
                12,
                34,
                KeyModifiers::NONE
            )))
        );
    }

    #[test]
    fn translate_carries_modifiers_alongside_the_mouse_action() {
        let event = mouse_event(CtMouseKind::ScrollUp, 0, 0, CtModifiers::CONTROL);

        assert_eq!(
            translate(event),
            Some(Event::Mouse(MouseEvent::new(
                MouseEventKind::ScrollUp,
                0,
                0,
                KeyModifiers::CONTROL
            ))),
            "Ctrl+scroll must stay distinguishable from a bare scroll"
        );
    }

    #[test]
    fn translate_keeps_a_move_with_no_button_held() {
        // `Moved` is the one kind carrying no button at all: it must survive
        // translation rather than be mistaken for an unsupported event.
        let event = mouse_event(CtMouseKind::Moved, 5, 6, CtModifiers::NONE);

        assert_eq!(
            translate(event),
            Some(Event::Mouse(MouseEvent::new(MouseEventKind::Moved, 5, 6, KeyModifiers::NONE)))
        );
    }

    #[test]
    fn accepts_every_event_while_the_mouse_is_captured() {
        let config = EventStreamConfig::default();
        let click = Event::Mouse(MouseEvent::new(
            MouseEventKind::Down(MouseButton::Left),
            1,
            2,
            KeyModifiers::NONE,
        ));
        let press = Event::KeyPress(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));

        assert!(accepts(&click, config));
        assert!(accepts(&press, config));
    }

    #[test]
    fn accepts_drops_only_mouse_events_once_capture_is_off() {
        // The terminal may still report the mouse after an opt-out, for
        // example when a crashed application left reporting on.
        let config = EventStreamConfig::default().mouse_capture(false);
        let click = Event::Mouse(MouseEvent::new(
            MouseEventKind::Down(MouseButton::Left),
            1,
            2,
            KeyModifiers::NONE,
        ));
        let press = Event::KeyPress(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));

        assert!(!accepts(&click, config));
        assert!(accepts(&press, config));
    }
}
