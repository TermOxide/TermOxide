//! Integration tests for ViewNode tree construction and rendering.
//!
//! Tests the ViewNode component covering:
//! - tree building with containers and leaves
//! - component ID attachment
//! - text and raw content nodes
//! - area validation
//! - tree structure integrity

use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Style;

use termoxide_rendering::view_node::{ComponentId, ViewNode};

// ─────────────────────────────────────────────────────────────────────────── //
//  Basic construction tests
// ─────────────────────────────────────────────────────────────────────────── //

#[test]
fn test_view_node_text_basic() {
    let rect = Rect::new(5, 10, 30, 1);
    let text = "Hello, World!";
    let style = Style::default();

    let node = ViewNode::text(rect, text, style);

    assert_eq!(node.area, rect);
    assert!(node.children.is_empty());
}

#[test]
fn test_view_node_container_basic() {
    let rect = Rect::new(0, 0, 80, 24);
    let children = vec![];

    let node = ViewNode::container(rect, children);

    assert_eq!(node.area, rect);
    assert!(node.children.is_empty());
}

#[test]
fn test_view_node_container_with_children() {
    let parent_rect = Rect::new(0, 0, 80, 24);
    let child1_rect = Rect::new(0, 0, 40, 12);
    let child2_rect = Rect::new(40, 0, 40, 12);

    let child1 = ViewNode::text(child1_rect, "Child 1", Style::default());
    let child2 = ViewNode::text(child2_rect, "Child 2", Style::default());

    let parent = ViewNode::container(parent_rect, vec![child1, child2]);

    assert_eq!(parent.area, parent_rect);
    assert_eq!(parent.children.len(), 2);
}

// ─────────────────────────────────────────────────────────────────────────── //
//  Component ID tests
// ─────────────────────────────────────────────────────────────────────────── //

#[test]
fn test_view_node_with_id() {
    let rect = Rect::new(0, 0, 40, 12);
    let id: ComponentId = 42;

    let node = ViewNode::text(rect, "Component", Style::default()).with_id(id);

    assert_eq!(node.id, Some(id));
}

#[test]
fn test_view_node_without_id() {
    let rect = Rect::new(0, 0, 40, 12);
    let node = ViewNode::text(rect, "No ID", Style::default());

    assert_eq!(node.id, None);
}

#[test]
fn test_view_node_id_overwrite() {
    let rect = Rect::new(0, 0, 40, 12);
    let id1: ComponentId = 1;
    let id2: ComponentId = 2;

    let node = ViewNode::text(rect, "Test", Style::default())
        .with_id(id1)
        .with_id(id2);

    assert_eq!(node.id, Some(id2));
}

#[test]
fn test_view_node_container_with_ids() {
    let child1 = ViewNode::text(Rect::new(0, 0, 40, 12), "C1", Style::default()).with_id(1);
    let child2 = ViewNode::text(Rect::new(40, 0, 40, 12), "C2", Style::default()).with_id(2);

    let parent = ViewNode::container(Rect::new(0, 0, 80, 24), vec![child1, child2]);

    assert_eq!(parent.children.len(), 2);
    assert_eq!(parent.children[0].id, Some(1));
    assert_eq!(parent.children[1].id, Some(2));
}

// ─────────────────────────────────────────────────────────────────────────── //
//  Area validation tests
// ─────────────────────────────────────────────────────────────────────────── //

#[test]
fn test_view_node_area_coverage() {
    let rect = Rect::new(0, 0, 80, 24);
    let node = ViewNode::text(rect, "Full screen", Style::default());

    assert_eq!(node.area.x, 0);
    assert_eq!(node.area.y, 0);
    assert_eq!(node.area.width, 80);
    assert_eq!(node.area.height, 24);
}

#[test]
fn test_view_node_area_offset() {
    let rect = Rect::new(10, 5, 50, 15);
    let node = ViewNode::text(rect, "Offset area", Style::default());

    assert_eq!(node.area.x, 10);
    assert_eq!(node.area.y, 5);
    assert_eq!(node.area.width, 50);
    assert_eq!(node.area.height, 15);
}

#[test]
fn test_view_node_area_minimum() {
    let rect = Rect::new(0, 0, 1, 1);
    let node = ViewNode::text(rect, "X", Style::default());

    assert_eq!(node.area.width, 1);
    assert_eq!(node.area.height, 1);
}

#[test]
fn test_view_node_area_zero() {
    let rect = Rect::new(0, 0, 0, 0);
    let node = ViewNode::text(rect, "Empty", Style::default());

    assert_eq!(node.area.width, 0);
    assert_eq!(node.area.height, 0);
}

// ─────────────────────────────────────────────────────────────────────────── //
//  Tree structure tests
// ─────────────────────────────────────────────────────────────────────────── //

#[test]
fn test_view_node_deeply_nested() {
    let level3 = ViewNode::text(Rect::new(20, 10, 10, 2), "Level 3", Style::default());

    let level2 = ViewNode::container(Rect::new(15, 8, 20, 4), vec![level3]);

    let level1 = ViewNode::container(Rect::new(10, 5, 30, 8), vec![level2]);

    let root = ViewNode::container(Rect::new(0, 0, 50, 20), vec![level1]);

    assert_eq!(root.children.len(), 1);
    assert_eq!(root.children[0].children.len(), 1);
    assert_eq!(root.children[0].children[0].children.len(), 1);
}

#[test]
fn test_view_node_wide_tree() {
    let mut children = Vec::new();
    for i in 0..10 {
        let x = (i * 8) as u16;
        let child = ViewNode::text(Rect::new(x, 0, 8, 10), &format!("C{}", i), Style::default())
            .with_id(i as u64 + 1);
        children.push(child);
    }

    let parent = ViewNode::container(Rect::new(0, 0, 80, 10), children);

    assert_eq!(parent.children.len(), 10);
    for i in 0..10 {
        assert_eq!(parent.children[i].id, Some(i as u64 + 1));
    }
}

// ─────────────────────────────────────────────────────────────────────────── //
//  Content type tests
// ─────────────────────────────────────────────────────────────────────────── //

#[test]
fn test_view_node_text_content() {
    let rect = Rect::new(0, 0, 20, 1);
    let text_content = "Hello";
    let node = ViewNode::text(rect, text_content, Style::default());

    assert_eq!(node.area, rect);
    assert!(node.children.is_empty());
}

#[test]
fn test_view_node_raw_content() {
    let rect = Rect::new(0, 0, 20, 10);

    let node = ViewNode::raw(rect, |buf, rect| {
        let _ = (buf, rect);
    });

    assert_eq!(node.area, rect);
    assert!(node.children.is_empty());
}

#[test]
fn test_view_node_raw_with_id() {
    let rect = Rect::new(5, 5, 30, 10);
    let id: ComponentId = 99;

    let node = ViewNode::raw(rect, |_buf, _rect| {
    })
    .with_id(id);

    assert_eq!(node.id, Some(id));
}

// ─────────────────────────────────────────────────────────────────────────── //
//  Children manipulation tests
// ─────────────────────────────────────────────────────────────────────────── //

#[test]
fn test_view_node_children_empty_initially() {
    let rect = Rect::new(0, 0, 80, 24);
    let node = ViewNode::container(rect, vec![]);

    assert_eq!(node.children.len(), 0);
}

#[test]
fn test_view_node_children_order_preserved() {
    let ids = vec![10, 20, 30, 40, 50];
    let children: Vec<_> = ids
        .iter()
        .enumerate()
        .map(|(i, &id)| {
            ViewNode::text(Rect::new(0, i as u16, 20, 1), "Item", Style::default()).with_id(id)
        })
        .collect();

    let parent = ViewNode::container(Rect::new(0, 0, 20, 5), children);

    for (i, &expected_id) in ids.iter().enumerate() {
        assert_eq!(parent.children[i].id, Some(expected_id));
    }
}

// ─────────────────────────────────────────────────────────────────────────── //
//  Mixed tree tests
// ─────────────────────────────────────────────────────────────────────────── //

#[test]
fn test_view_node_mixed_content_types() {
    let text_node =
        ViewNode::text(Rect::new(0, 0, 40, 5), "Text Area", Style::default()).with_id(1);

    let raw_node = ViewNode::raw(Rect::new(40, 0, 40, 5), |_buf, _rect| {}).with_id(2);

    let container = ViewNode::container(Rect::new(0, 0, 80, 10), vec![text_node, raw_node]);

    assert_eq!(container.children.len(), 2);
    assert_eq!(container.children[0].id, Some(1));
    assert_eq!(container.children[1].id, Some(2));
}

#[test]
fn test_view_node_complex_layout() {
    // Simulate a typical app layout: header, sidebar + main, footer

    let header = ViewNode::text(Rect::new(0, 0, 80, 2), "Header", Style::default()).with_id(1);

    let sidebar = ViewNode::text(Rect::new(0, 2, 20, 20), "Sidebar", Style::default()).with_id(2);

    let main = ViewNode::text(Rect::new(20, 2, 60, 20), "Main", Style::default()).with_id(3);

    let content = ViewNode::container(Rect::new(0, 2, 80, 20), vec![sidebar, main]);

    let footer = ViewNode::text(Rect::new(0, 22, 80, 2), "Footer", Style::default()).with_id(4);

    let app = ViewNode::container(Rect::new(0, 0, 80, 24), vec![header, content, footer]);

    assert_eq!(app.children.len(), 3);
    assert_eq!(app.children[0].id, Some(1)); // header
    assert_eq!(app.children[1].children.len(), 2); // content has sidebar + main
    assert_eq!(app.children[2].id, Some(4)); // footer
}

// ─────────────────────────────────────────────────────────────────────────── //
//  Area consistency tests
// ─────────────────────────────────────────────────────────────────────────── //

#[test]
fn test_view_node_child_within_parent_bounds() {
    let parent_rect = Rect::new(0, 0, 80, 24);
    let child_rect = Rect::new(10, 5, 30, 10);

    let child = ViewNode::text(child_rect, "Child", Style::default());
    let parent = ViewNode::container(parent_rect, vec![child]);

    assert!(parent.children[0].area.x >= parent.area.x);
    assert!(parent.children[0].area.y >= parent.area.y);
}

#[test]
fn test_view_node_child_exceeds_parent_bounds() {
    let parent_rect = Rect::new(0, 0, 40, 10);
    let child_rect = Rect::new(30, 5, 20, 10);

    let child = ViewNode::text(child_rect, "Child", Style::default());
    let parent = ViewNode::container(parent_rect, vec![child]);

    assert_eq!(parent.children[0].area.width, 20);
}
