use std::{cell::Cell, rc::Rc};

use termoxide_layout::{
    style::{Color, Dirty, Property, Style, Unit},
    tree::{NodeId, StyleTree, TreeError},
};

fn fg(tree: &StyleTree, id: NodeId) -> Property<Color> {
    tree.node(id).map_or(Property::Unset, |n| n.presented_style.fg)
}

fn record_shared(tree: &StyleTree, a: NodeId, b: NodeId) -> bool {
    match (tree.node(a), tree.node(b)) {
        (Some(a), Some(b)) => Rc::ptr_eq(&a.inherited, &b.inherited),
        _ => false,
    }
}

/// root → panel → { text, badge (overrides fg) → label }
struct Fixture {
    tree: StyleTree,
    root: NodeId,
    panel: NodeId,
    text: NodeId,
    badge: NodeId,
    label: NodeId,
}

fn fixture() -> Fixture {
    let mut tree = StyleTree::new();
    let root = tree.insert(None, Style::new().fg(Color::Base16(7))).unwrap();
    let panel = tree.insert(Some(root), Style::new().width(Unit::Cells(20))).unwrap();
    let text = tree.insert(Some(panel), Style::new()).unwrap();
    let badge = tree.insert(Some(panel), Style::new().fg(Color::Base16(1))).unwrap();
    let label = tree.insert(Some(badge), Style::new().bold(true)).unwrap();
    Fixture { tree, root, panel, text, badge, label }
}

#[test]
fn insert_resolves_and_marks_the_node_dirty() {
    let mut f = fixture();
    assert_eq!(fg(&f.tree, f.text), Property::Value(Color::Base16(7)));
    assert_eq!(f.tree.node(f.panel).map(|n| n.children.clone()), Some(vec![f.text, f.badge]));
    assert_eq!(f.tree.take_dirty(f.text), Ok(Dirty { paint: true, layout: true }));
    assert_eq!(f.tree.take_dirty(f.text), Ok(Dirty::default()));
}

#[test]
fn only_overriding_nodes_allocate_a_record() {
    let f = fixture();
    assert!(record_shared(&f.tree, f.root, f.panel));
    assert!(record_shared(&f.tree, f.root, f.text));
    assert!(!record_shared(&f.tree, f.panel, f.badge));
    assert!(!record_shared(&f.tree, f.badge, f.label));
}

#[test]
fn a_write_reaches_descendants_only_at_resolve() {
    let mut f = fixture();
    let dirty = f.tree.set_own_style(f.root, Style::new().fg(Color::Base16(2))).unwrap();
    assert_eq!(dirty, Dirty { paint: true, layout: false });
    assert_eq!(f.tree.node(f.root).map(|n| n.subtree_paint_dirty), Some(true));
    assert_eq!(fg(&f.tree, f.text), Property::Value(Color::Base16(7)));

    // panel, text and badge re-derive; badge keeps its own fg, so the descent
    // stops before label.
    assert_eq!(f.tree.resolve(), 3);
    assert_eq!(fg(&f.tree, f.root), Property::Value(Color::Base16(2)));
    assert_eq!(fg(&f.tree, f.text), Property::Value(Color::Base16(2)));
    assert_eq!(fg(&f.tree, f.label), Property::Value(Color::Base16(1)));
    assert!(record_shared(&f.tree, f.root, f.text));
    assert_eq!(f.tree.node(f.root).map(|n| n.subtree_paint_dirty), Some(false));
}

#[test]
fn resolve_skips_untouched_subtrees() {
    let mut f = fixture();
    assert_eq!(f.tree.resolve(), 0);

    f.tree.set_own_style(f.badge, Style::new().fg(Color::Base16(3))).unwrap();
    assert_eq!(f.tree.resolve(), 1);
    assert_eq!(fg(&f.tree, f.label), Property::Value(Color::Base16(3)));
}

#[test]
fn a_layout_write_stops_at_children_that_do_not_inherit_it() {
    let mut f = fixture();
    f.tree.set_own_style(f.panel, Style::new().width(Unit::Cells(30))).unwrap();
    assert_eq!(f.tree.resolve(), 2);
    for child in [f.text, f.badge] {
        assert_eq!(f.tree.node(child).map(|n| n.subtree_paint_dirty), Some(false));
    }
    f.tree.take_dirty(f.text).unwrap();
    f.tree.set_own_style(f.panel, Style::new().width(Unit::Cells(40))).unwrap();
    f.tree.resolve();
    assert_eq!(f.tree.take_dirty(f.text), Ok(Dirty { paint: true, layout: false }));
}

#[test]
fn explicit_layout_inherit_follows_the_parent_after_resolve() {
    let mut f = fixture();
    let follower = f
        .tree
        .insert(Some(f.panel), Style { width: Property::Inherit, ..Style::new() })
        .unwrap();
    f.tree.take_dirty(follower).unwrap();

    f.tree.set_own_style(f.panel, Style::new().width(Unit::Cells(30))).unwrap();
    let width = |tree: &StyleTree| tree.node(follower).map(|n| n.presented_style.width);
    assert_eq!(width(&f.tree), Some(Property::Value(Unit::Cells(20))));
    f.tree.resolve();
    assert_eq!(width(&f.tree), Some(Property::Value(Unit::Cells(30))));
    assert_eq!(f.tree.take_dirty(follower), Ok(Dirty { paint: true, layout: true }));
}

#[test]
fn presented_style_follows_target_at_resolve() {
    let mut f = fixture();
    f.tree.set_own_style(f.text, Style::new().italic(true)).unwrap();
    let italic = |tree: &StyleTree| tree.node(f.text).map(|n| (n.target_style.italic, n.presented_style.italic));
    assert_eq!(italic(&f.tree), Some((Property::Value(true), Property::Unset)));
    f.tree.resolve();
    assert_eq!(italic(&f.tree), Some((Property::Value(true), Property::Value(true))));
}

#[test]
fn removed_nodes_are_stale_and_mark_the_parent() {
    let mut f = fixture();
    f.tree.take_dirty(f.panel).unwrap();
    f.tree.remove(f.badge).unwrap();

    assert!(f.tree.node(f.label).is_none());
    assert_eq!(f.tree.node(f.panel).map(|n| n.children.clone()), Some(vec![f.text]));
    assert_eq!(f.tree.take_dirty(f.panel), Ok(Dirty { paint: false, layout: true }));
    assert_eq!(f.tree.set_own_style(f.label, Style::new()), Err(TreeError::StaleNode(f.label)));
    assert_eq!(f.tree.insert(Some(f.badge), Style::new()), Err(TreeError::StaleNode(f.badge)));
    assert_eq!(f.tree.remove(f.badge), Err(TreeError::StaleNode(f.badge)));
    assert_eq!(f.tree.take_dirty(f.badge), Err(TreeError::StaleNode(f.badge)));
    assert!(f.tree.set_style_effect(f.badge, Box::new(())).is_err());
    assert_eq!(
        TreeError::StaleNode(f.badge).to_string(),
        format!("style node {:?} was removed", f.badge)
    );
}

#[test]
fn a_removed_write_then_resolve_is_harmless() {
    let mut f = fixture();
    f.tree.set_own_style(f.badge, Style::new().fg(Color::Base16(5))).unwrap();
    f.tree.remove(f.panel).unwrap();
    assert_eq!(f.tree.resolve(), 0);
}

struct DropCounter(Rc<Cell<u32>>);

impl Drop for DropCounter {
    fn drop(&mut self) { self.0.set(self.0.get() + 1); }
}

#[test]
fn removing_a_subtree_drops_its_effects() {
    let mut f = fixture();
    let drops = Rc::new(Cell::new(0));
    for id in [f.badge, f.label, f.text] {
        f.tree.set_style_effect(id, Box::new(DropCounter(Rc::clone(&drops)))).unwrap();
    }

    f.tree.remove(f.badge).unwrap();
    assert_eq!(drops.get(), 2);

    f.tree.set_style_effect(f.text, Box::new(())).unwrap();
    assert_eq!(drops.get(), 3);
}
