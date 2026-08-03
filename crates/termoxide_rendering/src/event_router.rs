//! Routes [`termoxide_event`] events to the appropriate component.
//!
//! It maintains a component hit map and applies key-to-signal bindings.

use ratatui::layout::Rect;
use termoxide_event::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use termoxide_reactive::Signal;

use crate::view_node::{ComponentId, ViewNode};

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
    pub const fn new(code: KeyCode, modifiers: KeyModifiers) -> Self { Self { code, modifiers } }

    fn matches(self, key: KeyEvent) -> bool { self.code == key.code && self.modifiers == key.modifiers }
}

type SignalAction = Box<dyn Fn() + Send + Sync>;

/// Maps key presses to signal updates.
#[derive(Default)]
pub struct KeySignalBindings {
    bindings: Vec<(KeyBinding, SignalAction)>,
}

impl std::fmt::Debug for KeySignalBindings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeySignalBindings").field("len", &self.bindings.len()).finish()
    }
}

impl KeySignalBindings {
    /// Create an empty key binding table.
    pub fn new() -> Self { Self::default() }

    /// Number of installed bindings.
    pub fn len(&self) -> usize { self.bindings.len() }

    /// Returns `true` when no binding is installed.
    pub fn is_empty(&self) -> bool { self.bindings.is_empty() }

    /// Bind a key to a fixed signal value.
    pub fn bind_set<T>(&mut self, key: KeyBinding, signal: Signal<T>, value: T)
    where
        T: Clone + Send + Sync + 'static,
    {
        self.bindings.push((key, Box::new(move || signal.set(value.clone()))));
    }

    /// Bind a key to an in-place signal update closure.
    pub fn bind_update<T, F>(&mut self, key: KeyBinding, signal: Signal<T>, updater: F)
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

    /// Apply one event and return the number of updates.
    pub fn apply_event(&self, event: &Event) -> usize {
        match *event {
            Event::KeyPress(key) => {
                let mut updates = 0;
                for (binding, action) in &self.bindings {
                    if binding.matches(key) {
                        action();
                        updates += 1;
                    }
                }
                updates
            },
            // Bindings match on a key and its modifiers, so nothing here can
            // be triggered by the mouse or by the stream handshake.
            Event::ChannelReady | Event::Mouse(_) => 0,
        }
    }

    /// Apply multiple events and return the total number of updates.
    pub fn apply_events(&self, events: &[Event]) -> usize { events.iter().map(|event| self.apply_event(event)).sum() }
}

/// An entry in the spatial hit map.
#[derive(Debug, Clone)]
struct HitEntry {
    #[allow(dead_code)]
    area: Rect,
    #[allow(dead_code)]
    id: ComponentId,
}

/// Routes input events to components.
#[derive(Debug)]
pub struct EventRouter {
    hit_map: Vec<HitEntry>,

    focused: Option<ComponentId>,

    key_bindings: KeySignalBindings,
}

impl EventRouter {
    /// Create an empty event router.
    pub fn new() -> Self { Self { hit_map: Vec::new(), focused: None, key_bindings: KeySignalBindings::new() } }

    /// Rebuild the hit map from a view tree.
    pub fn sync_hit_map(&mut self, root: &ViewNode) {
        self.hit_map.clear();
        Self::collect_hit_entries(root, &mut self.hit_map);
    }

    fn collect_hit_entries(node: &ViewNode, entries: &mut Vec<HitEntry>) {
        if let Some(id) = node.id {
            entries.push(HitEntry { area: node.area, id });
        }
        for child in &node.children {
            Self::collect_hit_entries(child, entries);
        }
    }

    /// Bind a key to a fixed signal value.
    pub fn bind_key_set<T>(&mut self, key: KeyBinding, signal: Signal<T>, value: T)
    where
        T: Clone + Send + Sync + 'static,
    {
        self.key_bindings.bind_set(key, signal, value);
    }

    /// Bind a key to an update closure.
    pub fn bind_key_update<T, F>(&mut self, key: KeyBinding, signal: Signal<T>, updater: F)
    where
        T: Send + Sync + 'static,
        F: Fn(&mut T) + Send + Sync + 'static,
    {
        self.key_bindings.bind_update(key, signal, updater);
    }

    /// Mutable access to the underlying key bindings table.
    pub fn key_bindings_mut(&mut self) -> &mut KeySignalBindings { &mut self.key_bindings }

    /// Route an event and return its target component, if any.
    pub fn route_event(&mut self, event: &Event, _root: &ViewNode) -> Option<ComponentId> {
        match *event {
            Event::KeyPress(key) => self.route_key(key),
            // Hit-testing is not implemented yet, so a mouse action has no
            // target: see the module-level "Current limitations".
            Event::Mouse(_) => None,
            // Stream handshake — no target, nothing to bind.
            Event::ChannelReady => None,
        }
    }

    /// Route a keyboard event.
    fn route_key(&mut self, key: KeyEvent) -> Option<ComponentId> {
        let _ = self.key_bindings.apply_event(&Event::KeyPress(key));
        self.focused
    }
}

impl Default for EventRouter {
    fn default() -> Self { Self::new() }
}

/// Returns `true` if the terminal cell at (`col`, `row`) falls inside `rect`.
///
/// The rectangle is inclusive on the top-left and exclusive on the
/// bottom-right, matching ratatui's convention (`Rect::contains`).
#[inline]
#[allow(dead_code)]
fn contains(rect: Rect, col: u16, row: u16) -> bool {
    col >= rect.x
        && col < rect.x.saturating_add(rect.width)
        && row >= rect.y
        && row < rect.y.saturating_add(rect.height)
}

#[cfg(test)]
mod tests {
    use ratatui::style::Style;
    use termoxide_event::event::{MouseButton, MouseEvent, MouseEventKind};
    use termoxide_reactive::{Signal, with_owner};

    use super::*;

    fn make_tree() -> ViewNode {
        let left = ViewNode::text(Rect::new(0, 0, 40, 24), "left", Style::default()).with_id(1);
        let right = ViewNode::text(Rect::new(40, 0, 40, 24), "right", Style::default()).with_id(2);
        ViewNode::container(Rect::new(0, 0, 80, 24), vec![left, right])
    }

    /// Shorthand for a key press event with no modifier held.
    fn press(code: KeyCode) -> Event { Event::KeyPress(KeyEvent::new(code, KeyModifiers::NONE)) }

    #[test]
    fn key_bindings_are_applied_by_router() {
        with_owner(|| {
            let root = make_tree();
            let mut router = EventRouter::new();
            let signal = Signal::new(0i32);

            router.bind_key_set(KeyBinding::new(KeyCode::Char('k'), KeyModifiers::NONE), signal, 7);

            assert_eq!(router.route_event(&press(KeyCode::Char('k')), &root), None);
            assert_eq!(signal.get_untracked(), 7);
        });
    }

    #[test]
    fn contains_edge_cases() {
        let r = Rect::new(10, 5, 20, 10);
        assert!(contains(r, 10, 5));
        assert!(contains(r, 29, 14));
        assert!(!contains(r, 30, 5));
        assert!(!contains(r, 10, 15));
    }

    #[test]
    fn channel_ready_is_routed_nowhere_and_triggers_no_binding() {
        with_owner(|| {
            let root = make_tree();
            let mut router = EventRouter::new();
            let signal = Signal::new(0i32);

            router.bind_key_update(KeyBinding::new(KeyCode::Char('k'), KeyModifiers::NONE), signal, |value| {
                *value += 1
            });

            assert_eq!(router.route_event(&Event::ChannelReady, &root), None);
            assert_eq!(signal.get_untracked(), 0);
        });
    }

    #[test]
    fn mouse_events_are_routed_nowhere_and_trigger_no_binding() {
        with_owner(|| {
            let root = make_tree();
            let mut router = EventRouter::new();
            router.sync_hit_map(&root);
            let signal = Signal::new(0i32);

            router.bind_key_update(KeyBinding::new(KeyCode::Char('k'), KeyModifiers::NONE), signal, |value| {
                *value += 1
            });

            // The click lands inside the left component, which has an id and
            // sits in the hit map — routing still yields `None` until
            // hit-testing is implemented.
            let click = Event::Mouse(MouseEvent::new(
                MouseEventKind::Down(MouseButton::Left),
                10,
                10,
                KeyModifiers::NONE,
            ));

            assert_eq!(router.route_event(&click, &root), None);
            assert_eq!(signal.get_untracked(), 0);
        });
    }

    #[test]
    fn contains_near_u16_max_does_not_wrap() {
        let r = Rect::new(u16::MAX - 1, u16::MAX - 1, 10, 10);
        assert!(contains(r, u16::MAX - 1, u16::MAX - 1));
        assert!(!contains(r, u16::MAX, u16::MAX));
    }

    #[test]
    fn route_event_with_no_matching_bindings_returns_none() {
        let root = make_tree();
        let mut router = EventRouter::new();

        assert_eq!(router.route_event(&press(KeyCode::Char('k')), &root), None);
    }

    #[test]
    fn bind_set_updates_signal_when_key_matches() {
        with_owner(|| {
            let signal = Signal::new(0i32);
            let mut bindings = KeySignalBindings::new();

            bindings.bind_set(KeyBinding::new(KeyCode::Char('k'), KeyModifiers::NONE), signal, 42);

            let updates = bindings.apply_event(&press(KeyCode::Char('k')));

            assert_eq!(updates, 1);
            assert_eq!(signal.get_untracked(), 42);
        });
    }

    #[test]
    fn bind_update_ignores_non_matching_keys() {
        with_owner(|| {
            let signal = Signal::new(10i32);
            let mut bindings = KeySignalBindings::new();

            bindings.bind_update(KeyBinding::new(KeyCode::Char('k'), KeyModifiers::NONE), signal, |value| {
                *value += 1
            });

            let updates = bindings.apply_event(&press(KeyCode::Char('x')));

            assert_eq!(updates, 0);
            assert_eq!(signal.get_untracked(), 10);
        });
    }

    #[test]
    fn bindings_discriminate_on_modifiers() {
        with_owner(|| {
            let signal = Signal::new(0i32);
            let mut bindings = KeySignalBindings::new();

            bindings.bind_set(KeyBinding::new(KeyCode::Char('c'), KeyModifiers::CONTROL), signal, 1);

            // A bare 'c' must not fire a Ctrl+C binding...
            assert_eq!(bindings.apply_event(&press(KeyCode::Char('c'))), 0);
            assert_eq!(signal.get_untracked(), 0);

            // ...and an extra held modifier must not either, since matching is
            // on the exact modifier set.
            let ctrl_shift_c =
                Event::KeyPress(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL | KeyModifiers::SHIFT));
            assert_eq!(bindings.apply_event(&ctrl_shift_c), 0);

            let ctrl_c = Event::KeyPress(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
            assert_eq!(bindings.apply_event(&ctrl_c), 1);
            assert_eq!(signal.get_untracked(), 1);
        });
    }

    #[test]
    fn matching_key_can_drive_multiple_signals() {
        with_owner(|| {
            let a = Signal::new(0i32);
            let b = Signal::new(0i32);
            let mut bindings = KeySignalBindings::new();
            let key = KeyBinding::new(KeyCode::Char('j'), KeyModifiers::NONE);

            bindings.bind_update(key, a, |value| *value += 1);
            bindings.bind_update(key, b, |value| *value += 10);

            let updates = bindings.apply_event(&press(KeyCode::Char('j')));

            assert_eq!(updates, 2);
            assert_eq!(a.get_untracked(), 1);
            assert_eq!(b.get_untracked(), 10);
        });
    }

    #[test]
    fn apply_events_counts_updates_across_many_events() {
        with_owner(|| {
            let signal = Signal::new(0usize);
            let mut bindings = KeySignalBindings::new();

            bindings.bind_update(KeyBinding::new(KeyCode::Char('a'), KeyModifiers::NONE), signal, |value| {
                *value += 1
            });

            let events = vec![
                press(KeyCode::Char('a')),
                press(KeyCode::Char('x')),
                Event::ChannelReady,
                press(KeyCode::Char('a')),
            ];

            let updates = bindings.apply_events(&events);

            assert_eq!(updates, 2);
            assert_eq!(signal.get_untracked(), 2);
        });
    }

    #[test]
    fn collect_hit_entries_finds_all_components() {
        let root = make_tree();
        let mut entries = Vec::new();
        EventRouter::collect_hit_entries(&root, &mut entries);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, 1);
        assert_eq!(entries[1].id, 2);
    }

    #[test]
    fn sync_hit_map_builds_correct_mapping() {
        let root = make_tree();
        let mut router = EventRouter::new();
        router.sync_hit_map(&root);
        assert_eq!(router.hit_map.len(), 2);
    }
}
