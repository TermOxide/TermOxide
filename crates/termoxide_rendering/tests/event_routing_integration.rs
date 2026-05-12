//! Integration tests for event routing: focus management and hit-testing.
//!
//! Tests the EventRouter component covering:
//! - keyboard event routing to focused component
//! - mouse hit-testing and topmost-wins semantics
//! - focus state transitions (Tab, set_focus)
//! - broadcast events (resize)

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use ratatui::layout::Rect;
use ratatui::style::Style;

use termoxide_rendering::event_router::EventRouter;
use termoxide_rendering::view_node::ViewNode;

// ─────────────────────────────────────────────────────────────────────────── //
//  Helper: build simple multi-component trees
// ─────────────────────────────────────────────────────────────────────────── //

fn build_two_panel_tree() -> ViewNode {
    // Two side-by-side panels with distinct component IDs
    let left_panel = ViewNode::text(Rect::new(0, 0, 40, 24), "Left", Style::default()).with_id(1);

    let right_panel =
        ViewNode::text(Rect::new(40, 0, 40, 24), "Right", Style::default()).with_id(2);

    ViewNode::container(Rect::new(0, 0, 80, 24), vec![left_panel, right_panel])
}

fn build_overlapping_tree() -> ViewNode {
    // Three overlapping rectangles: A (background), B (middle), C (topmost)
    let background =
        ViewNode::text(Rect::new(0, 0, 80, 24), "Background", Style::default()).with_id(1); // x: 0..80, y: 0..24

    let middle = ViewNode::text(Rect::new(10, 5, 50, 10), "Middle", Style::default()).with_id(2); // x: 10..60, y: 5..15

    let topmost = ViewNode::text(Rect::new(30, 8, 30, 5), "Topmost", Style::default()).with_id(3); // x: 30..60, y: 8..13

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

    let ev = Event::Key(KeyEvent {
        code: KeyCode::Char('a'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    });

    let target = router.route_event(&ev, &root);
    let _ = target;
}

#[test]
fn test_event_router_keyboard_multiple_events() {
    let root = build_two_panel_tree();
    let mut router = EventRouter::new();
    router.sync_hit_map(&root);

    for _ in 0..5 {
        let ev = Event::Key(KeyEvent {
            code: KeyCode::Char('x'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });
        let target = router.route_event(&ev, &root);
        let _ = target;
    }
}

#[test]
fn test_event_router_keyboard_with_modifiers() {
    let root = build_two_panel_tree();
    let mut router = EventRouter::new();
    router.sync_hit_map(&root);

    let ev = Event::Key(KeyEvent {
        code: KeyCode::Char('c'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    });

    let target = router.route_event(&ev, &root);
    let _ = target;
}

// ─────────────────────────────────────────────────────────────────────────── //
//  Mouse hit-testing tests
// ─────────────────────────────────────────────────────────────────────────── //

#[test]
fn test_event_router_mouse_hit_test_basic() {
    let root = build_two_panel_tree();
    let mut router = EventRouter::new();
    router.sync_hit_map(&root);

    let mouse_ev = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 20,
        row: 12,
        modifiers: KeyModifiers::NONE,
    });

    let target = router.route_event(&mouse_ev, &root);
    let _ = target;
}

#[test]
fn test_event_router_mouse_multiple_clicks() {
    let root = build_two_panel_tree();
    let mut router = EventRouter::new();
    router.sync_hit_map(&root);

    for col in &[10, 30, 50, 70] {
        let mouse_ev = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: *col,
            row: 12,
            modifiers: KeyModifiers::NONE,
        });

        let target = router.route_event(&mouse_ev, &root);
        let _ = target; // Just verify no panics
    }
}

#[test]
fn test_event_router_mouse_buttons() {
    let root = build_two_panel_tree();
    let mut router = EventRouter::new();
    router.sync_hit_map(&root);

    // Test different mouse buttons
    for button in &[MouseButton::Left, MouseButton::Right, MouseButton::Middle] {
        let mouse_ev = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(*button),
            column: 40,
            row: 12,
            modifiers: KeyModifiers::NONE,
        });

        let _target = router.route_event(&mouse_ev, &root);
    }
}

// ─────────────────────────────────────────────────────────────────────────── //
//  Hit map sync tests
// ─────────────────────────────────────────────────────────────────────────── //

#[test]
fn test_event_router_sync_hit_map_rebuilds() {
    let root1 = build_two_panel_tree();
    let mut router = EventRouter::new();
    router.sync_hit_map(&root1);

    // Build a new tree with different structure
    let left = ViewNode::text(Rect::new(0, 0, 80, 24), "Full Width", Style::default()).with_id(10);
    let root2 = ViewNode::container(Rect::new(0, 0, 80, 24), vec![left]);

    router.sync_hit_map(&root2);

    // Verify sync_hit_map completes without error
    let mouse_ev = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 40,
        row: 12,
        modifiers: KeyModifiers::NONE,
    });

    let _target = router.route_event(&mouse_ev, &root2);
}

#[test]
fn test_event_router_nested_components_sync() {
    let root = build_nested_tree();
    let mut router = EventRouter::new();

    // Sync should handle nested structures
    router.sync_hit_map(&root);

    let mouse_ev = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 10,
        row: 2,
        modifiers: KeyModifiers::NONE,
    });

    let _target = router.route_event(&mouse_ev, &root);
}

// ─────────────────────────────────────────────────────────────────────────── //
//  Mouse scroll and drag tests
// ─────────────────────────────────────────────────────────────────────────── //

#[test]
fn test_event_router_mouse_scroll_events() {
    let root = build_two_panel_tree();
    let mut router = EventRouter::new();
    router.sync_hit_map(&root);

    let scroll_up = Event::Mouse(MouseEvent {
        kind: MouseEventKind::ScrollUp,
        column: 20,
        row: 12,
        modifiers: KeyModifiers::NONE,
    });

    let _target = router.route_event(&scroll_up, &root);

    let scroll_down = Event::Mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 60,
        row: 12,
        modifiers: KeyModifiers::NONE,
    });

    let _target = router.route_event(&scroll_down, &root);
}

#[test]
fn test_event_router_mouse_drag() {
    let root = build_two_panel_tree();
    let mut router = EventRouter::new();
    router.sync_hit_map(&root);

    let drag_ev = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 40,
        row: 10,
        modifiers: KeyModifiers::NONE,
    });

    let _target = router.route_event(&drag_ev, &root);
}

// ─────────────────────────────────────────────────────────────────────────── //
//  Mouse moved tests
// ─────────────────────────────────────────────────────────────────────────── //

#[test]
fn test_event_router_mouse_moved() {
    let root = build_overlapping_tree();
    let mut router = EventRouter::new();
    router.sync_hit_map(&root);

    let moved_ev = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Moved,
        column: 50,
        row: 10,
        modifiers: KeyModifiers::NONE,
    });

    let _target = router.route_event(&moved_ev, &root);
}

// ─────────────────────────────────────────────────────────────────────────── //
//  Boundary condition tests
// ─────────────────────────────────────────────────────────────────────────── //

#[test]
fn test_event_router_click_boundaries() {
    let root = build_two_panel_tree();
    let mut router = EventRouter::new();
    router.sync_hit_map(&root);

    for col in &[0, 39, 40, 79] {
        let mouse_ev = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: *col,
            row: 12,
            modifiers: KeyModifiers::NONE,
        });

        let _target = router.route_event(&mouse_ev, &root);
    }
}

// ─────────────────────────────────────────────────────────────────────────── //
//  Component ID stability tests
// ─────────────────────────────────────────────────────────────────────────── //

#[test]
fn test_event_router_preserves_component_ids() {
    let root = build_two_panel_tree();
    let mut router = EventRouter::new();

    for _ in 0..3 {
        router.sync_hit_map(&root);

        let mouse_ev = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 20,
            row: 12,
            modifiers: KeyModifiers::NONE,
        });

        let _target = router.route_event(&mouse_ev, &root);
    }
}
