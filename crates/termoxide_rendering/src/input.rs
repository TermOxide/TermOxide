//! Input/event utilities for TermOxide rendering.
//!
//! This module provides:
//! - a minimal framework-level [`Event`] type (currently key presses only),
//! - timed blocking crossterm polling via [`poll_events`],
//! - key-to-signal bindings backed by `termoxide_reactive::ArcRwSignal`.
//!
//! In full render-loop applications, the canonical keyboard handling entry
//! point is [`crate::event_router::EventRouter::route_event`], which can apply
//! these bindings as part of routing.

use std::time::Duration;

use crossterm::event::{self, Event as CrosstermEvent, KeyCode, KeyEventKind, KeyModifiers};
use termoxide_reactive::ArcRwSignal;

/// Default event polling timeout used by [`poll_events`].
///
/// A short timeout keeps idle CPU usage low while preserving responsive input.
pub const DEFAULT_POLL_TIMEOUT: Duration = Duration::from_millis(16);

/// Framework-level key press event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyPress {
    /// Key code emitted by crossterm.
    pub code: KeyCode,
    /// Active key modifiers for this press.
    pub modifiers: KeyModifiers,
}

impl KeyPress {
    /// Build a key press descriptor.
    pub const fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }
}

/// Minimal event surface exposed by this crate.
///
/// For now we only surface key press events, which is enough to wire hotkeys
/// to reactive signals. Non-key and non-press crossterm events are ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// A key press (not key repeat/release).
    KeyPress(KeyPress),
}

impl Event {
    /// Convert a crossterm event into a framework event.
    ///
    /// Returns `None` for non-key events and for key repeat/release events.
    pub fn from_crossterm(event: CrosstermEvent) -> Option<Self> {
        match event {
            CrosstermEvent::Key(key_event) if key_event.kind == KeyEventKind::Press => Some(
                Self::KeyPress(KeyPress::new(key_event.code, key_event.modifiers)),
            ),
            _ => None,
        }
    }
}

/// Poll events with a small blocking timeout.
///
/// This is the stable high-level polling entry point for applications that use
/// the lightweight framework [`Event`] type. It waits up to
/// [`DEFAULT_POLL_TIMEOUT`] for the first event, then drains all ready events.
///
/// Terminal I/O errors are intentionally swallowed and surfaced as an empty
/// batch to keep call sites simple.
pub fn poll_events() -> Vec<Event> {
    poll_events_timeout(DEFAULT_POLL_TIMEOUT).unwrap_or_default()
}

/// Poll events with a caller-provided timeout.
///
/// The first event wait uses `timeout`; if one event is found, this function
/// drains all remaining ready events with a zero timeout.
pub(crate) fn poll_events_timeout(timeout: Duration) -> std::io::Result<Vec<Event>> {
    let raw = poll_crossterm_events(timeout)?;
    Ok(to_framework_events(raw))
}

pub(crate) fn poll_crossterm_events(timeout: Duration) -> std::io::Result<Vec<CrosstermEvent>> {
    drain_events_with(timeout, event::poll, event::read)
}

fn to_framework_events(raw: Vec<CrosstermEvent>) -> Vec<Event> {
    raw.into_iter().filter_map(Event::from_crossterm).collect()
}

fn drain_events_with<PollFn, ReadFn>(
    timeout: Duration,
    mut poll: PollFn,
    mut read: ReadFn,
) -> std::io::Result<Vec<CrosstermEvent>>
where
    PollFn: FnMut(Duration) -> std::io::Result<bool>,
    ReadFn: FnMut() -> std::io::Result<CrosstermEvent>,
{
    let mut events = Vec::new();

    if !poll(timeout)? {
        return Ok(events);
    }

    events.push(read()?);

    while poll(Duration::from_millis(0))? {
        events.push(read()?);
    }

    Ok(events)
}

/// Keyboard matcher used by [`KeySignalBindings`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyBinding {
    /// Key code to match.
    pub code: KeyCode,
    /// Required modifiers to match.
    pub modifiers: KeyModifiers,
}

impl KeyBinding {
    /// Build a key binding.
    pub const fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }

    fn matches(self, press: KeyPress) -> bool {
        self.code == press.code && self.modifiers == press.modifiers
    }
}

type SignalAction = Box<dyn Fn() + Send + Sync>;

/// Maps key presses to signal updates.
///
/// Use this to wire specific hotkeys to specific reactive signals.
#[derive(Default)]
pub struct KeySignalBindings {
    bindings: Vec<(KeyBinding, SignalAction)>,
}

impl std::fmt::Debug for KeySignalBindings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeySignalBindings")
            .field("len", &self.bindings.len())
            .finish()
    }
}

impl KeySignalBindings {
    /// Create an empty key binding table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of installed bindings.
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    /// Returns `true` when no binding is installed.
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    /// Bind a key to a fixed signal value.
    pub fn bind_set<T>(&mut self, key: KeyBinding, signal: ArcRwSignal<T>, value: T)
    where
        T: Clone + Send + Sync + 'static,
    {
        self.bindings
            .push((key, Box::new(move || signal.set(value.clone()))));
    }

    /// Bind a key to an in-place signal update closure.
    pub fn bind_update<T, F>(&mut self, key: KeyBinding, signal: ArcRwSignal<T>, updater: F)
    where
        T: Send + Sync + 'static,
        F: Fn(&mut T) + Send + Sync + 'static,
    {
        self.bindings.push((
            key,
            Box::new(move || {
                signal.update(|value| updater(value));
            }),
        ));
    }

    /// Apply one framework event to all matching key bindings.
    ///
    /// Returns the number of signal updates performed.
    pub fn apply_event(&self, event: &Event) -> usize {
        match *event {
            Event::KeyPress(press) => {
                let mut updates = 0;
                for (binding, action) in &self.bindings {
                    if binding.matches(press) {
                        action();
                        updates += 1;
                    }
                }
                updates
            }
        }
    }

    /// Apply multiple events in order.
    ///
    /// Returns the total number of signal updates performed.
    pub fn apply_events(&self, events: &[Event]) -> usize {
        events.iter().map(|event| self.apply_event(event)).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyEventState, MouseButton, MouseEvent, MouseEventKind};
    use std::io::{Error, ErrorKind};
    use termoxide_reactive::with_owner;

    fn key_event(code: KeyCode, modifiers: KeyModifiers, kind: KeyEventKind) -> CrosstermEvent {
        CrosstermEvent::Key(KeyEvent {
            code,
            modifiers,
            kind,
            state: KeyEventState::NONE,
        })
    }

    #[test]
    fn from_crossterm_only_accepts_key_press_kind() {
        let press = Event::from_crossterm(key_event(
            KeyCode::Char('x'),
            KeyModifiers::NONE,
            KeyEventKind::Press,
        ));
        let repeat = Event::from_crossterm(key_event(
            KeyCode::Char('x'),
            KeyModifiers::NONE,
            KeyEventKind::Repeat,
        ));
        let release = Event::from_crossterm(key_event(
            KeyCode::Char('x'),
            KeyModifiers::NONE,
            KeyEventKind::Release,
        ));

        assert_eq!(
            press,
            Some(Event::KeyPress(KeyPress::new(
                KeyCode::Char('x'),
                KeyModifiers::NONE
            )))
        );
        assert_eq!(repeat, None);
        assert_eq!(release, None);
    }

    #[test]
    fn from_crossterm_ignores_non_key_events() {
        let mouse = CrosstermEvent::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 3,
            row: 7,
            modifiers: KeyModifiers::NONE,
        });

        assert_eq!(Event::from_crossterm(mouse), None);
    }

    #[test]
    fn drain_events_with_drains_all_ready_events() {
        let mut poll_results = vec![Ok(true), Ok(true), Ok(false)].into_iter();
        let mut read_results = vec![
            Ok(key_event(
                KeyCode::Char('a'),
                KeyModifiers::NONE,
                KeyEventKind::Press,
            )),
            Ok(key_event(
                KeyCode::Char('b'),
                KeyModifiers::SHIFT,
                KeyEventKind::Press,
            )),
        ]
        .into_iter();

        let events = drain_events_with(
            Duration::from_millis(25),
            |_| poll_results.next().unwrap_or(Ok(false)),
            || {
                read_results
                    .next()
                    .unwrap_or_else(|| Err(Error::new(ErrorKind::UnexpectedEof, "no event")))
            },
        )
        .expect("drain should succeed");

        assert_eq!(events.len(), 2);
    }

    #[test]
    fn drain_events_with_propagates_poll_error() {
        let result = drain_events_with(
            Duration::from_millis(1),
            |_| Err(Error::new(ErrorKind::BrokenPipe, "poll failed")),
            || {
                Ok(key_event(
                    KeyCode::Char('a'),
                    KeyModifiers::NONE,
                    KeyEventKind::Press,
                ))
            },
        );

        assert!(result.is_err());
        assert_eq!(
            result.expect_err("expected error").kind(),
            ErrorKind::BrokenPipe
        );
    }

    #[test]
    fn drain_events_with_propagates_read_error() {
        let result = drain_events_with(
            Duration::from_millis(1),
            |_| Ok(true),
            || Err(Error::new(ErrorKind::UnexpectedEof, "read failed")),
        );

        assert!(result.is_err());
        assert_eq!(
            result.expect_err("expected error").kind(),
            ErrorKind::UnexpectedEof
        );
    }

    #[test]
    fn bind_set_updates_signal_when_key_matches() {
        with_owner(|| {
            let signal = ArcRwSignal::new(0i32);
            let mut bindings = KeySignalBindings::new();

            bindings.bind_set(
                KeyBinding::new(KeyCode::Char('k'), KeyModifiers::NONE),
                signal.clone(),
                42,
            );

            let updates = bindings.apply_event(&Event::KeyPress(KeyPress::new(
                KeyCode::Char('k'),
                KeyModifiers::NONE,
            )));

            assert_eq!(updates, 1);
            assert_eq!(signal.get_untracked(), 42);
        });
    }

    #[test]
    fn bind_update_ignores_non_matching_keys() {
        with_owner(|| {
            let signal = ArcRwSignal::new(10i32);
            let mut bindings = KeySignalBindings::new();

            bindings.bind_update(
                KeyBinding::new(KeyCode::Char('k'), KeyModifiers::NONE),
                signal.clone(),
                |value| *value += 1,
            );

            let updates = bindings.apply_event(&Event::KeyPress(KeyPress::new(
                KeyCode::Char('x'),
                KeyModifiers::NONE,
            )));

            assert_eq!(updates, 0);
            assert_eq!(signal.get_untracked(), 10);
        });
    }

    #[test]
    fn matching_key_can_drive_multiple_signals() {
        with_owner(|| {
            let a = ArcRwSignal::new(0i32);
            let b = ArcRwSignal::new(0i32);
            let mut bindings = KeySignalBindings::new();
            let key = KeyBinding::new(KeyCode::Char('j'), KeyModifiers::NONE);

            bindings.bind_update(key, a.clone(), |value| *value += 1);
            bindings.bind_update(key, b.clone(), |value| *value += 10);

            let updates = bindings.apply_event(&Event::KeyPress(KeyPress::new(
                KeyCode::Char('j'),
                KeyModifiers::NONE,
            )));

            assert_eq!(updates, 2);
            assert_eq!(a.get_untracked(), 1);
            assert_eq!(b.get_untracked(), 10);
        });
    }

    #[test]
    fn apply_events_counts_updates_across_many_events() {
        with_owner(|| {
            let signal = ArcRwSignal::new(0usize);
            let mut bindings = KeySignalBindings::new();

            bindings.bind_update(
                KeyBinding::new(KeyCode::Char('a'), KeyModifiers::NONE),
                signal.clone(),
                |value| *value += 1,
            );

            let events = vec![
                Event::KeyPress(KeyPress::new(KeyCode::Char('a'), KeyModifiers::NONE)),
                Event::KeyPress(KeyPress::new(KeyCode::Char('x'), KeyModifiers::NONE)),
                Event::KeyPress(KeyPress::new(KeyCode::Char('a'), KeyModifiers::NONE)),
            ];

            let updates = bindings.apply_events(&events);

            assert_eq!(updates, 2);
            assert_eq!(signal.get_untracked(), 2);
        });
    }
}
