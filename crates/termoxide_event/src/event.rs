//! # Event vocabulary — backend-agnostic input types
//!
//! This module defines the types the rest of TermOxide reacts to: [`Event`]
//! and its building blocks [`KeyEvent`], [`KeyCode`] and [`KeyModifiers`].
//! They deliberately contain **no** reference to any terminal backend (such as
//! `crossterm`); the translation from a concrete backend into these types
//! lives entirely in [`crate::backend`]. Keeping this boundary here is what
//! lets the framework swap or support several backends without touching
//! consumer code.

use std::ops::{BitOr, BitOrAssign};

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
}
