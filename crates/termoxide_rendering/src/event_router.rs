//! Routes [`termoxide_event`] events to the appropriate component.
//!
//! [`EventRouter`] sits between the input stream (which emits untyped
//! [`Event`]s) and the component tree (which expects events targeted at a
//! specific [`ComponentId`]).  It answers the question *"which component should
//! receive this event?"*, and applies the global key-to-signal bindings on the
//! way through, so that keyboard handling has a single entry point.
//!
//! ## Hit map
//!
//! The hit map is an ordered list of `(Rect, ComponentId)` pairs built from
//! the live [`ViewNode`] tree by [`EventRouter::sync_hit_map`].  It is rebuilt
//! after every render pass so that moving or resizing components is always
//! reflected immediately.
//!
//! Building the hit map is O(nodes).
//!
//! ## Focus management
//!
//! Only one component holds focus at a time.  `None` focus means keyboard
//! events are routed to `None` (the application root can still handle them as
//! global hotkeys).
//!
//! ## Current limitations
//!
//! Focus is never assigned: there is no `set_focus` yet and no Tab / Shift-Tab
//! traversal, so [`EventRouter::route_event`] always returns `None` and the
//! hit map is built but not yet consulted.  Mouse hit-testing is likewise not
//! wired up — [`termoxide_event`] does not carry mouse events at all today.
//!
//! ## Example
//!
//! ```rust
//! use ratatui::layout::Rect;
//! use termoxide_event::event::{Event, KeyCode, KeyEvent, KeyModifiers};
//! use termoxide_rendering::{
//!     event_router::EventRouter,
//!     view_node::{ViewContent, ViewNode},
//! };
//!
//! // Build a minimal tree: a container with two labelled children.
//! let a = ViewNode::text(
//!     Rect::new(0, 0, 40, 12),
//!     "left",
//!     ratatui::style::Style::default(),
//! )
//! .with_id(1);
//! let b = ViewNode::text(
//!     Rect::new(40, 0, 40, 12),
//!     "right",
//!     ratatui::style::Style::default(),
//! )
//! .with_id(2);
//! let root = ViewNode::container(Rect::new(0, 0, 80, 24), vec![a, b]);
//!
//! let mut router = EventRouter::new();
//! router.sync_hit_map(&root); // build the spatial index
//!
//! // Keyboard events go to the focused component.
//! let key_ev = Event::KeyPress(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
//! assert_eq!(router.route_event(&key_ev, &root), None);
//! ```

use ratatui::layout::Rect;
use termoxide_event::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use termoxide_reactive::Signal;

use crate::view_node::{ComponentId, ViewNode};

// ───────────────────────────────────────────────────────────────────────────
// //  Key bindings
// ───────────────────────────────────────────────────────────────────────────
// //

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

    /// Matches on an exact modifier set, not a subset.
    ///
    /// `Ctrl+S` therefore does not fire on `Ctrl+Shift+S`, so a binding can
    /// never be triggered by accident through an extra held modifier.
    fn matches(self, key: KeyEvent) -> bool { self.code == key.code && self.modifiers == key.modifiers }
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

    /// Apply one event to all matching key bindings.
    ///
    /// Returns the number of signal updates performed.
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
            Event::ChannelReady => 0,
        }
    }

    /// Apply multiple events in order.
    ///
    /// Returns the total number of signal updates performed.
    pub fn apply_events(&self, events: &[Event]) -> usize { events.iter().map(|event| self.apply_event(event)).sum() }
}

// ───────────────────────────────────────────────────────────────────────────
// //  HitEntry
// ───────────────────────────────────────────────────────────────────────────
// //

/// A single entry in the spatial hit map.
///
/// Holds the terminal area occupied by a component and its [`ComponentId`].
/// Entries are stored in document order so that reverse iteration implements
/// "topmost wins" hit-testing (last-drawn = visually on top).
#[derive(Debug, Clone)]
struct HitEntry {
    /// Terminal area (inclusive bounding box) of the component.
    area: Rect,
    /// Stable component identifier.
    id: ComponentId,
}

// ───────────────────────────────────────────────────────────────────────────
// //  EventRouter
// ───────────────────────────────────────────────────────────────────────────
// //

/// Routes raw crossterm [`Event`]s to the component that should handle them.
///
/// See the [module-level documentation][self] for a full description of the
/// routing algorithm and focus management.
#[derive(Debug)]
pub struct EventRouter {
    /// Ordered list of focusable areas, updated by
    /// [`sync_hit_map`][Self::sync_hit_map].
    hit_map: Vec<HitEntry>,

    /// The [`ComponentId`] of the component that currently holds keyboard
    /// focus.
    ///
    /// `None` when no component is focused (global hotkey mode).
    focused: Option<ComponentId>,

    /// Global key bindings executed from routed key presses.
    key_bindings: KeySignalBindings,
}

impl EventRouter {
    /// Create an empty `EventRouter` with no focus.
    ///
    /// Call [`sync_hit_map`][Self::sync_hit_map] after the first render pass
    /// to populate the spatial index.
    pub fn new() -> Self {
        Self {
            hit_map: Vec::new(),
            focused: None,
            key_bindings: KeySignalBindings::new(),
        }
    }

    // ── Hit-map management ───────────────────────────────────────────────────
    // //

    /// Rebuild the spatial index from the current [`ViewNode`] tree.
    ///
    /// Must be called after every render pass during which the component tree
    /// changed (i.e. nodes moved, resized, added, or removed).
    ///
    /// The method performs a depth-first traversal and records every node that
    /// carries a [`ComponentId`] in document order.
    ///
    /// # Complexity
    ///
    /// O(nodes) in tree size.
    pub fn sync_hit_map(&mut self, root: &ViewNode) {
        self.hit_map.clear();
        Self::collect_hit_entries(root, &mut self.hit_map);
    }

    /// Depth-first collector — appends [`HitEntry`]s for nodes with an `id`.
    fn collect_hit_entries(node: &ViewNode, entries: &mut Vec<HitEntry>) {
        if let Some(id) = node.id {
            entries.push(HitEntry {
                area: node.area,
                id,
            });
        }
        for child in &node.children {
            Self::collect_hit_entries(child, entries);
        }
    }

    // ── Key-to-signal bindings ─────────────────────────────────────────────
    // //

    /// Bind a key to a fixed signal value.
    ///
    /// These bindings are executed from [`route_event`][Self::route_event], so
    /// keyboard handling has a single entry point.
    pub fn bind_key_set<T>(
        &mut self,
        key: KeyBinding,
        signal: Signal<T>,
        value: T,
    ) where
        T: Clone + Send + Sync + 'static,
    {
        self.key_bindings.bind_set(key, signal, value);
    }

    /// Bind a key to an in-place signal update closure.
    ///
    /// These bindings are executed from [`route_event`][Self::route_event], so
    /// keyboard handling has a single entry point.
    pub fn bind_key_update<T, F>(
        &mut self,
        key: KeyBinding,
        signal: Signal<T>,
        updater: F,
    ) where
        T: Send + Sync + 'static,
        F: Fn(&mut T) + Send + Sync + 'static,
    {
        self.key_bindings.bind_update(key, signal, updater);
    }

    /// Mutable access to the underlying key bindings table.
    pub fn key_bindings_mut(&mut self) -> &mut KeySignalBindings {
        &mut self.key_bindings
    }

    // ── Event routing ────────────────────────────────────────────────────────
    // //

    /// Determine which component should handle `event` and return its id.
    ///
    /// - **Key press**: applies the matching global bindings, then returns [`Self::focused`].
    /// - **[`Event::ChannelReady`]**: a stream handshake carrying no input, routed to `None`.
    ///
    /// Note that the input stream only ever reports key *presses*: releases and
    /// repeats are already filtered out by [`termoxide_event`], so there is no
    /// event kind left to check here.
    pub fn route_event(&mut self, event: &Event, _root: &ViewNode) -> Option<ComponentId> {
        match *event {
            Event::KeyPress(key) => self.route_key(key),
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

// ───────────────────────────────────────────────────────────────────────────
// //  Geometry helpers
// ───────────────────────────────────────────────────────────────────────────
// //

/// Returns `true` if the terminal cell at (`col`, `row`) falls inside `rect`.
///
/// The rectangle is inclusive on the top-left and exclusive on the
/// bottom-right, matching ratatui's convention (`Rect::contains`).
#[inline]
fn contains(rect: Rect, col: u16, row: u16) -> bool {
    col >= rect.x
        && col < rect.x.saturating_add(rect.width)
        && row >= rect.y
        && row < rect.y.saturating_add(rect.height)
}

// ───────────────────────────────────────────────────────────────────────────
// //  Tests
// ───────────────────────────────────────────────────────────────────────────
// //

#[cfg(test)]
mod tests {
    use ratatui::style::Style;
    use termoxide_reactive::{Signal, with_owner};

    use super::*;

    fn make_tree() -> ViewNode {
        let left =
            ViewNode::text(Rect::new(0, 0, 40, 24), "left", Style::default())
                .with_id(1);
        let right =
            ViewNode::text(Rect::new(40, 0, 40, 24), "right", Style::default())
                .with_id(2);
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

            router.bind_key_set(
                KeyBinding::new(KeyCode::Char('k'), KeyModifiers::NONE),
                signal,
                7,
            );

            assert_eq!(router.route_event(&press(KeyCode::Char('k')), &root), None);
            assert_eq!(signal.get_untracked(), 7);
        });
    }

    #[test]
    fn contains_edge_cases() {
        let r = Rect::new(10, 5, 20, 10);
        assert!(contains(r, 10, 5)); // top-left corner
        assert!(contains(r, 29, 14)); // bottom-right - 1
        assert!(!contains(r, 30, 5)); // right edge (exclusive)
        assert!(!contains(r, 10, 15)); // bottom edge (exclusive)
    }

    #[test]
    fn channel_ready_is_routed_nowhere_and_triggers_no_binding() {
        with_owner(|| {
            let root = make_tree();
            let mut router = EventRouter::new();
            let signal = Signal::new(0i32);

            router.bind_key_update(
                KeyBinding::new(KeyCode::Char('k'), KeyModifiers::NONE),
                signal,
                |value| *value += 1,
            );

            assert_eq!(router.route_event(&Event::ChannelReady, &root), None);
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

            bindings.bind_set(KeyBinding::new(KeyCode::Char('k'), KeyModifiers::NONE), signal.clone(), 42);

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

            bindings.bind_update(
                KeyBinding::new(KeyCode::Char('k'), KeyModifiers::NONE),
                signal.clone(),
                |value| *value += 1,
            );

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
            let ctrl_shift_c = Event::KeyPress(KeyEvent::new(
                KeyCode::Char('c'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            ));
            assert_eq!(bindings.apply_event(&ctrl_shift_c), 0);

            let ctrl_c =
                Event::KeyPress(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
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

            bindings.bind_update(key, a.clone(), |value| *value += 1);
            bindings.bind_update(key, b.clone(), |value| *value += 10);

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

            bindings.bind_update(
                KeyBinding::new(KeyCode::Char('a'), KeyModifiers::NONE),
                signal.clone(),
                |value| *value += 1,
            );

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
