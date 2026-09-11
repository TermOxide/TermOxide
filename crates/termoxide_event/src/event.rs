//! # Event vocabulary — backend-agnostic input types
//!
//! This module defines the types the rest of TermOxide reacts to: [`Event`]
//! and its building blocks [`KeyEvent`], [`KeyCode`] and [`KeyModifiers`].
//! They deliberately contain **no** reference to any terminal backend (such as
//! `crossterm`); the translation from a concrete backend into these types
//! lives entirely in [`crate::backend`]. Keeping this boundary here is what
//! lets the framework swap or support several backends without touching
//! consumer code.

use std::{
    fmt,
    ops::{BitOr, BitOrAssign},
};

/// A single keyboard key, independent of the terminal backend.
///
/// This mirrors the subset of keys TermOxide understands. Backend-specific
/// key codes are converted into this enum inside [`crate::backend`]; any key
/// without a matching variant is dropped rather than represented here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    /// Backspace key.
    Backspace,
    /// Enter key.
    Enter,
    /// Left arrow key.
    Left,
    /// Right arrow key.
    Right,
    /// Up arrow key.
    Up,
    /// Down arrow key.
    Down,
    /// Home key.
    Home,
    /// End key.
    End,
    /// Page up key.
    PageUp,
    /// Page down key.
    PageDown,
    /// Tab key.
    Tab,
    /// Shift + Tab key.
    BackTab,
    /// Delete key.
    Delete,
    /// Insert key.
    Insert,
    /// F key.
    ///
    /// `KeyCode::F(1)` represents F1 key, etc.
    F(u8),
    /// A character.
    ///
    /// `KeyCode::Char('c')` represents `c` character, etc.
    Char(char),
    /// Null.
    Null,
    /// Escape key.
    Esc,
}

/// The set of modifier keys held down during a key press.
///
/// This is a small bit set rather than an enum, because modifiers combine:
/// `Ctrl+Shift+S` is a single press carrying two modifiers. Backend-specific
/// modifier flags are converted into this type inside [`crate::backend`];
/// modifiers without a matching constant are dropped rather than represented
/// here.
///
/// The inner bits are private and the only ways to build a value are the
/// constants below and unions of them through [`BitOr`] — a set is therefore
/// never able to carry a bit that isn't one of the named modifiers, which is
/// what lets the rest of the crate (and [`Display`](fmt::Display)) treat every
/// bit as known.
///
/// ```
/// use termoxide_event::event::KeyModifiers;
///
/// let combo = KeyModifiers::CONTROL | KeyModifiers::SHIFT;
///
/// assert!(combo.contains(KeyModifiers::CONTROL));
/// assert!(!combo.contains(KeyModifiers::ALT));
/// assert!(KeyModifiers::NONE.is_empty());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct KeyModifiers(u8);

impl KeyModifiers {
    /// Alt (or Option) key held.
    pub const ALT: Self = Self(1 << 2);
    /// Control key held.
    pub const CONTROL: Self = Self(1 << 1);
    /// Every named modifier paired with its label, in [`Display`](fmt::Display) order.
    const NAMED: [(Self, &'static str); 4] = [
        (Self::SHIFT, "shift"),
        (Self::CONTROL, "control"),
        (Self::ALT, "alt"),
        (Self::SUPER, "super"),
    ];
    /// No modifier held.
    pub const NONE: Self = Self(0);
    /// Shift key held.
    pub const SHIFT: Self = Self(1 << 0);
    /// Super key held (Windows key, Command).
    pub const SUPER: Self = Self(1 << 3);

    /// Returns `true` when every modifier in `other` is also held here.
    ///
    /// [`NONE`](Self::NONE) is contained in any set, since it requires
    /// nothing.
    pub const fn contains(self, other: Self) -> bool { self.0 & other.0 == other.0 }

    /// Returns `true` when no modifier at all is held.
    pub const fn is_empty(self) -> bool { self.0 == 0 }
}

impl BitOr for KeyModifiers {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self { Self(self.0 | rhs.0) }
}

impl BitOrAssign for KeyModifiers {
    fn bitor_assign(&mut self, rhs: Self) { self.0 |= rhs.0; }
}

impl fmt::Display for KeyModifiers {
    /// Writes the held modifiers as `+`-separated lower-case names, or `none`
    /// when nothing is held.
    ///
    /// Only bits with an entry in [`NAMED`](Self::NAMED) can be set: the
    /// backend drops modifiers it has no constant for, and the inner `u8` is
    /// private, so there is no way to build a set this cannot name.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_empty() {
            return f.write_str("none");
        }

        let labels = Self::NAMED
            .iter()
            .filter(|(flag, _)| self.contains(*flag))
            .map(|(_, label)| *label);

        f.write_str(&labels.collect::<Vec<_>>().join("+"))
    }
}

/// A single key press: which key, and which modifiers were held with it.
///
/// Splitting the modifiers out of [`KeyCode`] is what lets consumers tell
/// `Ctrl+C` apart from a plain `c` — both share the same
/// [`KeyCode::Char('c')`](KeyCode::Char).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyEvent {
    /// The key that was pressed.
    pub code: KeyCode,
    /// Modifiers held down during the press.
    pub modifiers: KeyModifiers,
}

impl KeyEvent {
    /// Build a key press descriptor.
    pub const fn new(code: KeyCode, modifiers: KeyModifiers) -> Self { Self { code, modifiers } }
}

/// A mouse button, independent of the terminal backend.
///
/// Terminals only ever report these three buttons: an extra side button is
/// either reported as one of them or not reported at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    /// Left mouse button.
    Left,
    /// Right mouse button.
    Right,
    /// Middle mouse button, usually the wheel pressed down.
    Middle,
}

/// What a [`MouseEvent`] reports: a button action, a bare move, or a scroll.
///
/// Every kind a terminal can report has a variant here, so translating one
/// never loses an event — unlike [`KeyCode`], which drops the keys it cannot
/// name.
///
/// ## Unreported buttons
///
/// Some terminals do not say which button was involved in an [`Up`](Self::Up)
/// or a [`Drag`](Self::Drag). [`MouseButton::Left`] is reported in that case,
/// because that is all the backend knows; it is not evidence that the left
/// button was the one used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseEventKind {
    /// A button was pressed.
    Down(MouseButton),
    /// A button was released.
    Up(MouseButton),
    /// The pointer moved with a button held.
    Drag(MouseButton),
    /// The pointer moved with no button held.
    ///
    /// Reported for every cell the pointer crosses, which makes it by far the
    /// most frequent kind: an application that does not track hovering should
    /// discard it early.
    Moved,
    /// The wheel was scrolled up, away from the user.
    ScrollUp,
    /// The wheel was scrolled down, towards the user.
    ScrollDown,
    /// The wheel was tilted left, mostly on a laptop touchpad.
    ScrollLeft,
    /// The wheel was tilted right, mostly on a laptop touchpad.
    ScrollRight,
}

/// A single mouse action: what happened, where, and with which modifiers held.
///
/// The position is expressed in terminal cells, `(0, 0)` being the top-left
/// corner of the terminal. That is the same coordinate space components are
/// laid out in, so a position can be compared against their areas without any
/// conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MouseEvent {
    /// What the mouse did.
    pub kind: MouseEventKind,
    /// Column of the cell the pointer was over, counted from 0.
    pub column: u16,
    /// Row of the cell the pointer was over, counted from 0.
    pub row: u16,
    /// Modifiers held down during the action.
    ///
    /// Mouse actions share [`KeyModifiers`] with key presses, because a
    /// terminal reports the same modifier set for both.
    pub modifiers: KeyModifiers,
}

impl MouseEvent {
    /// Build a mouse action descriptor.
    pub const fn new(kind: MouseEventKind, column: u16, row: u16, modifiers: KeyModifiers) -> Self {
        Self { kind, column, row, modifiers }
    }
}

/// An event delivered by an [`EventStream`](crate::EventStream).
///
/// This is the single unit of communication flowing from the background
/// reader thread to the application through the stream's channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Event {
    /// Handshake emitted exactly once, as the very first event, when the
    /// reader thread has started and the channel is live.
    ///
    /// It carries no input; it lets a consumer block until the stream is
    /// ready before assuming further events will flow.
    ChannelReady,
    /// A key was pressed. Only key *presses* are reported — releases and
    /// repeats are filtered out by the backend.
    KeyPress(KeyEvent),
    /// The mouse was used: a button action, a move, or a scroll.
    ///
    /// Mouse reporting is a terminal mode the stream turns on for its own
    /// lifetime, so these only flow while an
    /// [`EventStream`](crate::EventStream) is alive.
    Mouse(MouseEvent),
}

#[cfg(test)]
mod tests {
    use super::KeyModifiers;

    #[test]
    fn key_modifiers_display_uses_readable_names() {
        assert_eq!(format!("{}", KeyModifiers::NONE), "none");
        assert_eq!(format!("{}", KeyModifiers::CONTROL), "control");
        assert_eq!(format!("{}", KeyModifiers::CONTROL | KeyModifiers::SHIFT), "shift+control");
    }

    #[test]
    fn key_modifiers_display_names_every_known_flag() {
        let all = KeyModifiers::SHIFT | KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER;

        assert_eq!(format!("{all}"), "shift+control+alt+super");
    }
}
