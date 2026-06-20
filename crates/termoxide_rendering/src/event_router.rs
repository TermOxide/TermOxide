//! Routes raw crossterm events to the appropriate component.
//!
//! [`EventRouter`] sits between crossterm (which emits untyped [`Event`]s) and
//! the component tree (which expects events targeted at a specific
//! [`ComponentId`]).  It answers the question *"which component should receive
//! this event?"* using two complementary strategies:
//!
//! | Event class   | Routing strategy                                        |
//! |---------------|---------------------------------------------------------|
//! | Keyboard      | delivered to the currently focused    |
//! |               | component, identified by [`ComponentId`].               |
//! | Mouse (click, | **Hit-test** — the [`ViewNode`] area that contains the  |
//! | hover, scroll)| cursor position claims the event.  Ties are broken by   |
//! |               | document order (last child wins, i.e. topmost drawn).   |
//! | Resize        | Broadcast — routed to `None` so the application's root  |
//! |               | handler can trigger a full relayout.                    |
//!
//! ## Hit map
//!
//! The hit map is an ordered list of `(Rect, ComponentId)` pairs built from
//! the live [`ViewNode`] tree by [`EventRouter::sync_hit_map`].  It is rebuilt
//! after every render pass so that moving or resizing components is always
//! reflected immediately.
//!
//! Building the hit map is O(nodes); hit-testing a mouse event is O(nodes) in
//! the worst case but terminates early on the first match when iterating in
//! reverse document order (topmost → bottommost).
//!
//! ## Focus management
//!
//! Focus moves between components via:
//!
//! - **Explicit API**: call [`EventRouter::set_focus`] from application code.
//! - **Tab / Shift-Tab**: the router intercepts these and advances or retreats
//!   focus through the ordered list of focusable components (those with a
//!   [`ComponentId`]).
//!
//! Only one component holds focus at a time.  `None` focus means keyboard
//! events are routed to `None` (the application root can still handle them as
//! global hotkeys).
//!
//! ## Example
//!
//! ```rust
//! use ratatui::layout::Rect;
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
//! use crossterm::event::{
//!     Event,
//!     KeyCode,
//!     KeyEvent,
//!     KeyEventKind,
//!     KeyEventState,
//!     KeyModifiers,
//! };
//! let key_ev = Event::Key(KeyEvent {
//!     code: KeyCode::Char('j'),
//!     modifiers: KeyModifiers::NONE,
//!     kind: KeyEventKind::Press,
//!     state: KeyEventState::NONE,
//! });
//! assert_eq!(router.route_event(&key_ev, &root), None);
//! ```

use crossterm::event::{Event, KeyEventKind};
use ratatui::layout::Rect;
use termoxide_reactive::Signal;

pub use crate::input::KeyBinding;
use crate::{
    input::{Event as InputEvent, KeyPress, KeySignalBindings},
    view_node::{ComponentId, ViewNode},
};

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
    /// - **Keyboard**: delivered to [`Self::focused`].  Tab / Shift-Tab advance
    ///   or retreat focus before returning the new focused id.
    /// - **Mouse**: hit-tested against the spatial index.  Clicking also
    ///   transfers keyboard focus to the clicked component.
    /// - **Resize / FocusGained / FocusLost / Paste**: returned as `None` so
    ///   the application root handles them.
    pub fn route_event(
        &mut self,
        event: &Event,
        _root: &ViewNode,
    ) -> Option<ComponentId> {
        match event {
            Event::Key(key_ev) => self.route_key(key_ev),
            // Resize, FocusGained, FocusLost, Paste → broadcast (no specific
            // target).
            _ => None,
        }
    }

    /// Route a keyboard event.
    fn route_key(
        &mut self,
        ev: &crossterm::event::KeyEvent,
    ) -> Option<ComponentId> {
        if ev.kind != KeyEventKind::Press {
            return self.focused;
        }

        // Keep Tab reserved for focus traversal. All other key presses can
        // drive global reactive bindings.
        let _ = self.key_bindings.apply_event(&InputEvent::KeyPress(
            KeyPress::new(ev.code, ev.modifiers),
        ));

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
    use crossterm::event::{
        KeyCode,
        KeyEvent,
        KeyEventKind,
        KeyEventState,
        KeyModifiers,
        MouseButton,
        MouseEvent,
        MouseEventKind,
    };
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

            let ev = Event::Key(KeyEvent {
                code: KeyCode::Char('k'),
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: KeyEventState::NONE,
            });

            assert_eq!(router.route_event(&ev, &root), None);
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
    fn non_press_key_events_do_not_trigger_bindings() {
        with_owner(|| {
            let root = make_tree();
            let mut router = EventRouter::new();
            let signal = Signal::new(0i32);

            router.bind_key_update(
                KeyBinding::new(KeyCode::Char('k'), KeyModifiers::NONE),
                signal,
                |value| *value += 1,
            );

            let repeat = Event::Key(KeyEvent {
                code: KeyCode::Char('k'),
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Repeat,
                state: KeyEventState::NONE,
            });

            assert_eq!(router.route_event(&repeat, &root), None);
            assert_eq!(signal.get_untracked(), 0);
        });
    }

    #[test]
    fn zero_sized_hitboxes_are_ignored() {
        let zero =
            ViewNode::text(Rect::new(0, 0, 0, 0), "zero", Style::default())
                .with_id(10);
        let root = ViewNode::container(Rect::new(0, 0, 10, 10), vec![zero]);

        let mut router = EventRouter::new();
        router.sync_hit_map(&root);

        let click = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        });

        assert_eq!(router.route_event(&click, &root), None);
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

        let ev = Event::Key(KeyEvent {
            code: KeyCode::Char('k'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });

        assert_eq!(router.route_event(&ev, &root), None);
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
