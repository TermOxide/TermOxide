use termoxide_layout::{
    driver::{DriverError, LayerSolver, drive, overlay_bounds},
    geometry::CellRect,
    style::{Property, Style, Unit},
    tree::{Layer, NodeId, StyleTree, TreeError},
};

const VIEWPORT: CellRect = CellRect::new(0, 0, 81, 25);
const OVERLAY: Option<(Layer, i32)> = Some((Layer::Overlay, 0));

/// Gives each layer root its bounds and each child the first row of its
/// parent, recording every call.
#[derive(Default)]
struct Recorder {
    calls: Vec<(NodeId, CellRect)>,
}

impl LayerSolver for Recorder {
    fn solve(&mut self, tree: &mut StyleTree, root: NodeId, bounds: CellRect) -> Result<(), TreeError> {
        self.calls.push((root, bounds));
        tree.set_rect(root, bounds)?;
        let children = tree.node(root).map(|n| n.children.clone()).unwrap_or_default();
        for child in children {
            if tree.node(child).is_some_and(|n| n.layer.is_none()) {
                tree.set_rect(child, CellRect { height: 1, ..bounds })?;
            }
        }
        Ok(())
    }
}

fn modal(width: u16, height: u16) -> Style {
    Style::new()
        .width(Unit::Cells(width.try_into().unwrap()))
        .height(Unit::Cells(height.try_into().unwrap()))
}

#[test]
fn base_is_solved_before_overlays_in_key_order() {
    let mut tree = StyleTree::new();
    let app = tree.insert(None, Style::new()).unwrap();
    let high = tree.insert(Some(app), modal(10, 4)).unwrap();
    let low = tree.insert(Some(app), modal(20, 5)).unwrap();
    let body = tree.insert(Some(low), Style::new()).unwrap();
    tree.set_layer(high, Some((Layer::Overlay, 1))).unwrap();
    tree.set_layer(low, OVERLAY).unwrap();

    let mut solver = Recorder::default();
    let order = drive(&mut tree, &mut solver, VIEWPORT).unwrap();

    assert_eq!(solver.calls, vec![
        (app, VIEWPORT),
        (low, CellRect::new(30, 10, 20, 5)),
        (high, CellRect::new(35, 10, 10, 4)),
    ]);
    let painted: Vec<NodeId> = order.iter().map(|i| i.id).collect();
    assert_eq!(painted, vec![app, low, body, high]);
}

#[test]
fn odd_leftover_space_goes_right_and_down() {
    let mut tree = StyleTree::new();
    let app = tree.insert(None, Style::new()).unwrap();
    let m = tree.insert(Some(app), modal(2, 2)).unwrap();
    tree.set_layer(m, OVERLAY).unwrap();
    let viewport = CellRect::new(4, 2, 7, 6);
    assert_eq!(overlay_bounds(&tree, viewport), Ok(vec![(m, CellRect::new(6, 4, 2, 2))]));
}

#[test]
fn an_oversized_modal_keeps_its_size() {
    let mut tree = StyleTree::new();
    let m = tree.insert(None, modal(100, 30)).unwrap();
    tree.set_layer(m, OVERLAY).unwrap();
    assert_eq!(overlay_bounds(&tree, VIEWPORT), Ok(vec![(m, CellRect::new(0, 0, 100, 30))]));
}

#[test]
fn a_modal_without_a_size_in_cells_is_an_error() {
    for style in [
        Style::new().width(Unit::Percent(50.0)).height(Unit::Cells(3)),
        Style::new().width(Unit::Cells(3)),
        Style { width: Property::Value(Unit::Cells(-1)), ..modal(1, 1) },
    ] {
        let mut tree = StyleTree::new();
        let m = tree.insert(None, style).unwrap();
        tree.set_layer(m, OVERLAY).unwrap();
        let error = drive(&mut tree, &mut Recorder::default(), VIEWPORT).unwrap_err();
        assert_eq!(error, DriverError::UnresolvedModalSize(m));
        assert_eq!(error.to_string(), format!("modal {m:?} needs a width and height in cells"));
    }
}

struct Failing;

impl LayerSolver for Failing {
    fn solve(&mut self, _: &mut StyleTree, root: NodeId, _: CellRect) -> Result<(), TreeError> {
        Err(TreeError::StaleNode(root))
    }
}

#[test]
fn solver_errors_are_returned() {
    let mut tree = StyleTree::new();
    let app = tree.insert(None, Style::new()).unwrap();
    let error = drive(&mut tree, &mut Failing, VIEWPORT).unwrap_err();
    assert_eq!(error, DriverError::Tree(TreeError::StaleNode(app)));
    assert_eq!(error.to_string(), TreeError::StaleNode(app).to_string());
}

#[test]
fn an_empty_tree_paints_nothing() {
    assert_eq!(drive(&mut StyleTree::new(), &mut Recorder::default(), VIEWPORT), Ok(Vec::new()));
}
