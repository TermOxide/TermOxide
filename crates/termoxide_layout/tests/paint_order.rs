use termoxide_layout::{
    geometry::CellRect,
    style::{BorderSides, Color, Display, Property, Style},
    tree::{Layer, NodeId, PaintItem, StyleTree},
};

const VIEWPORT: CellRect = CellRect::new(0, 0, 80, 24);
const OVERLAY: Option<(Layer, i32)> = Some((Layer::Overlay, 0));

fn node(tree: &mut StyleTree, parent: Option<NodeId>, style: Style, rect: CellRect) -> NodeId {
    let id = tree.insert(parent, style).unwrap();
    tree.set_rect(id, rect).unwrap();
    id
}

fn ids(items: &[PaintItem]) -> Vec<NodeId> { items.iter().map(|i| i.id).collect() }

fn item(items: &[PaintItem], id: NodeId) -> PaintItem {
    let Some(found) = items.iter().find(|i| i.id == id) else {
        panic!("{id:?} is not painted");
    };
    *found
}

#[test]
fn base_layer_paints_in_pre_order_inside_parent_padding_boxes() {
    let mut tree = StyleTree::new();
    let boxed = Style::new().border_sides(BorderSides::ALL);
    let root = node(&mut tree, None, boxed, CellRect::new(0, 0, 20, 10));
    let a = node(&mut tree, Some(root), Style::new(), CellRect::new(0, 0, 30, 3));
    let a1 = node(&mut tree, Some(a), Style::new(), CellRect::new(2, 1, 4, 1));
    let b = node(&mut tree, Some(root), Style::new(), CellRect::new(1, 4, 5, 2));

    let items = tree.paint_order(VIEWPORT);
    assert_eq!(ids(&items), vec![root, a, a1, b]);
    assert_eq!(item(&items, root).clip, CellRect::new(0, 0, 20, 10));
    assert_eq!(item(&items, a).clip, CellRect::new(1, 1, 18, 2));
    assert_eq!(item(&items, a1).clip, CellRect::new(2, 1, 4, 1));
    assert!(items.iter().all(|i| i.layer == (Layer::Base, 0)));
}

#[test]
fn a_portal_paints_after_the_base_layer_clipped_from_the_viewport() {
    let mut tree = StyleTree::new();
    let root = node(&mut tree, None, Style::new(), CellRect::new(0, 0, 10, 5));
    let modal = node(&mut tree, Some(root), Style::new(), CellRect::new(20, 8, 40, 8));
    let body = node(&mut tree, Some(modal), Style::new(), CellRect::new(22, 9, 10, 2));
    let after = node(&mut tree, Some(root), Style::new(), CellRect::new(0, 1, 2, 1));
    tree.set_layer(modal, OVERLAY).unwrap();

    let items = tree.paint_order(VIEWPORT);
    assert_eq!(ids(&items), vec![root, after, modal, body]);
    assert_eq!(item(&items, modal).clip, CellRect::new(20, 8, 40, 8));
    assert_eq!(item(&items, body).layer, (Layer::Overlay, 0));
    assert_eq!(tree.node(body).and_then(|n| n.layer), None);
}

#[test]
fn layers_paint_in_key_order_and_equal_keys_keep_tree_order() {
    let mut tree = StyleTree::new();
    let root = node(&mut tree, None, Style::new(), VIEWPORT);
    let rect = CellRect::new(1, 1, 1, 1);
    let top = node(&mut tree, Some(root), Style::new(), rect);
    let first = node(&mut tree, Some(root), Style::new(), rect);
    let nested = node(&mut tree, Some(first), Style::new(), rect);
    let second = node(&mut tree, Some(root), Style::new(), rect);
    tree.set_layer(top, Some((Layer::Overlay, 5))).unwrap();
    for id in [first, nested, second] {
        tree.set_layer(id, OVERLAY).unwrap();
    }

    assert_eq!(ids(&tree.paint_order(VIEWPORT)), vec![root, first, nested, second, top]);
}

#[test]
fn a_root_can_start_a_layer() {
    let mut tree = StyleTree::new();
    let rect = CellRect::new(0, 0, 4, 4);
    let toast = node(&mut tree, None, Style::new(), rect);
    let app = node(&mut tree, None, Style::new(), rect);
    tree.set_layer(toast, OVERLAY).unwrap();
    tree.set_layer(app, Some((Layer::Base, 0))).unwrap();
    assert_eq!(ids(&tree.paint_order(VIEWPORT)), vec![app, toast]);

    tree.set_layer(toast, None).unwrap();
    assert_eq!(ids(&tree.paint_order(VIEWPORT)), vec![toast, app]);
}

#[test]
fn hidden_and_unplaced_subtrees_are_skipped() {
    let mut tree = StyleTree::new();
    let rect = CellRect::new(0, 0, 4, 4);
    let root = node(&mut tree, None, Style::new(), rect);
    let hidden = node(&mut tree, Some(root), Style::new().display(Display::None), rect);
    let hidden_modal = node(&mut tree, Some(hidden), Style::new(), rect);
    let unplaced = tree.insert(Some(root), Style::new()).unwrap();
    node(&mut tree, Some(unplaced), Style::new(), rect);
    tree.set_layer(hidden_modal, OVERLAY).unwrap();

    assert_eq!(ids(&tree.paint_order(VIEWPORT)), vec![root]);
}

#[test]
fn a_portal_still_inherits_from_its_logical_parent() {
    let mut tree = StyleTree::new();
    let root = node(&mut tree, None, Style::new().fg(Color::Base16(6)), VIEWPORT);
    let modal = node(&mut tree, Some(root), Style::new(), VIEWPORT);
    tree.set_layer(modal, OVERLAY).unwrap();
    assert_eq!(
        tree.node(modal).map(|n| n.presented_style.fg),
        Some(Property::Value(Color::Base16(6)))
    );
}

#[test]
fn set_layer_marks_the_node_paint_dirty_only_when_it_changes() {
    let mut tree = StyleTree::new();
    let id = tree.insert(None, Style::new()).unwrap();
    tree.take_dirty(id).unwrap();
    tree.set_layer(id, None).unwrap();
    assert!(!tree.take_dirty(id).unwrap().paint);
    tree.set_layer(id, OVERLAY).unwrap();
    assert!(tree.take_dirty(id).unwrap().paint);
}

#[test]
fn removing_a_root_drops_it_from_the_paint_order() {
    let mut tree = StyleTree::new();
    let a = node(&mut tree, None, Style::new(), VIEWPORT);
    let b = node(&mut tree, None, Style::new(), VIEWPORT);
    tree.remove(a).unwrap();
    assert_eq!(ids(&tree.paint_order(VIEWPORT)), vec![b]);
}
