//! # Config — the settings an event stream starts with
//!
//! An [`EventStream`](crate::EventStream) puts the terminal into the modes it
//! reads input in. [`EventStreamConfig`] lets an application turn off the ones
//! it has no use for; the defaults are what
//! [`EventStream::new`](crate::EventStream::new) uses.

/// Settings an [`EventStream`](crate::EventStream) is started with.
///
/// Build one from [`Default`] and adjust it with its setters. The struct is
/// `#[non_exhaustive]`, so it cannot be written as a struct literal outside
/// this crate: a setting added later is then not a breaking change.
///
/// ```no_run
/// use termoxide_event::{EventStream, EventStreamConfig};
///
/// // Keyboard only: the terminal keeps handling text selection itself.
/// let events = EventStream::with_config(EventStreamConfig::default().mouse_capture(false));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct EventStreamConfig {
    /// Whether the terminal reports the mouse, as
    /// [`Event::Mouse`](crate::event::Event::Mouse). On by default.
    ///
    /// While it is on, the terminal stops handling text selection itself —
    /// most terminals still select with `Shift` held. An application that
    /// ignores the mouse can turn it off to give selection back.
    pub mouse_capture: bool,
}

impl EventStreamConfig {
    /// Turn mouse reporting on or off; see
    /// [`mouse_capture`](Self::mouse_capture).
    #[must_use]
    pub const fn mouse_capture(mut self, enabled: bool) -> Self {
        self.mouse_capture = enabled;
        self
    }
}

impl Default for EventStreamConfig {
    /// Every mode on: the mouse is reported.
    fn default() -> Self { Self { mouse_capture: true } }
}

#[cfg(test)]
mod tests {
    use super::EventStreamConfig;

    #[test]
    fn default_captures_the_mouse() {
        assert!(EventStreamConfig::default().mouse_capture);
    }

    #[test]
    fn mouse_capture_can_be_turned_off_and_back_on() {
        let off = EventStreamConfig::default().mouse_capture(false);
        assert!(!off.mouse_capture);

        assert_eq!(off.mouse_capture(true), EventStreamConfig::default());
    }
}
