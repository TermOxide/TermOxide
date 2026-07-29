//! # Event vocabulary — backend-agnostic input types
//!
//! This module defines the types the rest of TermOxide reacts to: [`Event`]
//! and its building block [`KeyCode`]. They deliberately contain **no**
//! reference to any terminal backend (such as `crossterm`); the translation
//! from a concrete backend into these types lives entirely in
//! [`crate::backend`]. Keeping this boundary here is what lets the framework
//! swap or support several backends without touching consumer code.

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

/// Keyboard modifiers attached to a key press.
pub use crossterm::event::KeyModifiers;

/// A backend-agnostic keyboard press.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyEvent {
    /// The key that was pressed.
    pub code: KeyCode,
    /// Active modifiers at the time of the press.
    pub modifiers: KeyModifiers,
}

impl KeyEvent {
    /// Create a new key press descriptor.
    pub const fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
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
}
