//! Integration tests for event routing: focus management and hit-testing.
//!
//! Tests the EventRouter component covering:
//! - keyboard event routing to focused component
//! - keyboard routing through the global bindings
//! - hit-map maintenance across varied and nested trees
//! - the stream handshake being routed nowhere
//!
//! Mouse hit-testing, focus transitions (Tab, `set_focus`) and resize broadcast
//! are not covered: the router implements none of them yet, and
//! `termoxide_event` carries neither mouse nor resize events today.

use termoxide_event::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{layout::Rect, style::Style};
use termoxide_rendering::{event_router::EventRouter, view_node::ViewNode};

// ─────────────────────────────────────────────────────────────────────────── //
//  Helper: build simple multi-component trees
// ─────────────────────────────────────────────────────────────────────────── //

fn build_two_panel_tree() -> ViewNode {
    // Two side-by-side panels with distinct component IDs
    let left_panel = ViewNode::text(Rect::new(0, 0, 40, 24), "Left", Style::default()).with_id(1);

    let right_panel = ViewNode::text(Rect::new(40, 0, 40, 24), "Right", Style::default()).with_id(2);

    ViewNode::container(Rect::new(0, 0, 80, 24), vec![left_panel, right_panel])
}

fn build_overlapping_tree() -> ViewNode {
    // Three overlapping rectangles: A (background), B (middle), C (topmost)
    // x: 0..80, y: 0..24
    let background = ViewNode::text(Rect::new(0, 0, 80, 24), "Background", Style::default()).with_id(1);

    // x: 10..60, y: 5..15
    let middle = ViewNode::text(Rect::new(10, 5, 50, 10), "Middle", Style::default()).with_id(2);

    // x: 30..60, y: 8..13
    let topmost = ViewNode::text(Rect::new(30, 8, 30, 5), "Topmost", Style::default()).with_id(3);

    ViewNode::container(Rect::new(0, 0, 80, 24), vec![background, middle, topmost])
}

fn build_nested_tree() -> ViewNode {
    // Deeply nested structure: container > child_container > leaf
    let leaf_a = ViewNode::text(Rect::new(0, 0, 20, 5), "Leaf A", Style::default()).with_id(1);

    let leaf_b = ViewNode::text(Rect::new(20, 0, 20, 5), "Leaf B", Style::default()).with_id(2);

    let child_container = ViewNode::container(Rect::new(0, 0, 40, 5), vec![leaf_a, leaf_b]);

    let leaf_c = ViewNode::text(Rect::new(0, 10, 40, 5), "Leaf C", Style::default()).with_id(3);

    ViewNode::container(Rect::new(0, 0, 80, 24), vec![child_container, leaf_c])
}

// ─────────────────────────────────────────────────────────────────────────── //
//  Focus management tests
// ─────────────────────────────────────────────────────────────────────────── //

#[test]
fn test_event_router_keyboard_basic() {
    let root = build_two_panel_tree();
    let mut router = EventRouter::new();
    router.sync_hit_map(&root);

    let ev = Event::KeyPress(KeyEvent { code: KeyCode::Char('a'), modifiers: KeyModifiers::NONE });

    let target = router.route_event(&ev, &root);
    assert!((target.is_none()));
}

#[test]
fn test_event_router_keyboard_multiple_events() {
    let root = build_two_panel_tree();
    let mut router = EventRouter::new();
    router.sync_hit_map(&root);

    for _ in 0..5 {
        let ev = Event::KeyPress(KeyEvent { code: KeyCode::Char('x'), modifiers: KeyModifiers::NONE });
        let target = router.route_event(&ev, &root);
        assert!(target.is_none());
    }
}

#[test]
fn test_event_router_keyboard_with_modifiers() {
    let root = build_two_panel_tree();
    let mut router = EventRouter::new();
    router.sync_hit_map(&root);

    let ev = Event::KeyPress(KeyEvent { code: KeyCode::Char('c'), modifiers: KeyModifiers::CONTROL });

    let target = router.route_event(&ev, &root);
    assert!(target.is_none());
}
// ─────────────────────────────────────────────────────────────────────────── //
//  Hit-map maintenance tests
// ─────────────────────────────────────────────────────────────────────────── //
//
//  The hit map is built but not yet consulted, so these only assert that
//  syncing it over varied trees keeps routing well-behaved. They will grow real
//  assertions once hit-testing lands.

#[test]
fn test_event_router_sync_hit_map_rebuilds() {
    let root1 = build_two_panel_tree();
    let mut router = EventRouter::new();
    router.sync_hit_map(&root1);

    // Build a new tree with a different structure.
    let left = ViewNode::text(Rect::new(0, 0, 80, 24), "Full Width", Style::default()).with_id(10);
    let root2 = ViewNode::container(Rect::new(0, 0, 80, 24), vec![left]);

    router.sync_hit_map(&root2);

    let ev = Event::KeyPress(KeyEvent { code: KeyCode::Char('a'), modifiers: KeyModifiers::NONE });

    let target = router.route_event(&ev, &root2);
    assert!(target.is_none());
}

#[test]
fn test_event_router_nested_components_sync() {
    let root = build_nested_tree();
    let mut router = EventRouter::new();

    // Sync should handle nested structures.
    router.sync_hit_map(&root);

    let ev = Event::KeyPress(KeyEvent { code: KeyCode::Char('a'), modifiers: KeyModifiers::NONE });

    let target = router.route_event(&ev, &root);
    assert!(target.is_none());
}

#[test]
fn test_event_router_overlapping_tree_sync() {
    let root = build_overlapping_tree();
    let mut router = EventRouter::new();

    router.sync_hit_map(&root);

    let ev = Event::KeyPress(KeyEvent { code: KeyCode::Char('a'), modifiers: KeyModifiers::NONE });

    let target = router.route_event(&ev, &root);
    assert!(target.is_none());
}

// ─────────────────────────────────────────────────────────────────────────── //
//  Stream handshake
// ─────────────────────────────────────────────────────────────────────────── //

#[test]
fn test_event_router_ignores_channel_ready() {
    let root = build_two_panel_tree();
    let mut router = EventRouter::new();
    router.sync_hit_map(&root);

    assert!(router.route_event(&Event::ChannelReady, &root).is_none());
}
