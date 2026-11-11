use termoxide_layout::{
    geometry::{CellRect, Corner, content_box, padding_box},
    style::{BorderSides, BorderStyle, Property, Style, Unit},
    tree::{StyleTree, TreeError},
};

#[test]
fn intersection_keeps_the_overlap() {
    let a = CellRect::new(0, 0, 10, 10);
    assert_eq!(a.intersection(CellRect::new(5, 2, 10, 4)), CellRect::new(5, 2, 5, 4));
    assert_eq!(a.intersection(a), a);
}

#[test]
fn disjoint_rects_intersect_to_an_empty_rect() {
    let gap = CellRect::new(0, 0, 4, 4).intersection(CellRect::new(6, 1, 2, 2));
    assert!(gap.is_empty());
    assert_eq!((gap.x, gap.y, gap.width), (6, 1, 0));
}

#[test]
fn edges_saturate_at_the_grid_limit() {
    let far = CellRect::new(u16::MAX - 2, 0, 10, 1);
    assert_eq!(far.right(), u16::MAX);
    assert_eq!(far.bottom(), 1);
    assert_eq!(far.intersection(CellRect::new(0, 0, u16::MAX, u16::MAX)).width, 2);
}

#[test]
fn shrinking_past_zero_saturates() {
    let r = CellRect::new(3, 3, 4, 2).shrink(5, 1, 5, 1);
    assert_eq!(r, CellRect::new(4, 5, 2, 0));
    assert!(r.is_empty());
}

#[test]
fn each_drawn_side_takes_one_cell_whatever_the_glyphs() {
    let rect = CellRect::new(0, 0, 10, 5);
    for style in [BorderStyle::Single, BorderStyle::None] {
        let boxed = Style::new().border_sides(BorderSides::ALL).border_style(style);
        assert_eq!(padding_box(&boxed, rect), CellRect::new(1, 1, 8, 3));
    }
    let top_left = Style::new().border_sides(BorderSides::TOP.union(BorderSides::LEFT));
    assert_eq!(padding_box(&top_left, rect), CellRect::new(1, 1, 9, 4));
    assert_eq!(padding_box(&Style::new(), rect), rect);
}

#[test]
fn content_box_subtracts_border_then_cell_padding() {
    let style = Style::new().border_sides(BorderSides::ALL).pad_x(2).padding_top(Unit::Cells(1));
    assert_eq!(content_box(&style, CellRect::new(0, 0, 20, 6)), CellRect::new(3, 2, 14, 3));
}

#[test]
fn unresolved_and_negative_padding_count_as_zero() {
    let style = Style {
        padding_left: Property::Value(Unit::Cells(-4)),
        padding_top: Property::Value(Unit::Percent(50.0)),
        ..Style::new()
    };
    let rect = CellRect::new(0, 0, 8, 8);
    assert_eq!(content_box(&style, rect), rect);
}

#[test]
fn tiny_boxes_never_go_negative() {
    let style = Style::new().border_sides(BorderSides::ALL).padding(3);
    assert_eq!(content_box(&style, CellRect::new(0, 0, 1, 1)).width, 0);
}

#[test]
fn unit_cells_resolves_cells_only() {
    assert_eq!(Unit::Cells(-2).cells(), Some(-2));
    for unresolved in [Unit::Percent(10.0), Unit::Fr(1.0), Unit::Auto] {
        assert_eq!(unresolved.cells(), None);
    }
}

#[test]
fn every_border_style_has_its_glyph_set() {
    let sets = [
        (BorderStyle::Single, "┌─┐│└┘"),
        (BorderStyle::Double, "╔═╗║╚╝"),
        (BorderStyle::Rounded, "╭─╮│╰╯"),
        (BorderStyle::Thick, "┏━┓┃┗┛"),
        (BorderStyle::Ascii, "+-+|++"),
        (BorderStyle::None, "      "),
    ];
    for (style, expected) in sets {
        let g = style.glyphs();
        let got: String = [g.top_left, g.horizontal, g.top_right, g.vertical, g.bottom_left, g.bottom_right]
            .into_iter()
            .collect();
        assert_eq!(got, expected, "{style:?}");
    }
}

#[test]
fn corners_need_both_adjacent_sides() {
    let g = BorderStyle::Single.glyphs();
    assert_eq!(g.corner(Corner::TopLeft, BorderSides::ALL), Some('┌'));
    assert_eq!(g.corner(Corner::BottomRight, BorderSides::ALL), Some('┘'));
    assert_eq!(g.corner(Corner::TopRight, BorderSides::TOP), Some('─'));
    assert_eq!(g.corner(Corner::BottomLeft, BorderSides::LEFT), Some('│'));
    assert_eq!(g.corner(Corner::TopLeft, BorderSides::BOTTOM.union(BorderSides::RIGHT)), None);
    assert_eq!(g.corner(Corner::TopLeft, BorderSides::NONE), None);
}

#[test]
fn the_tree_stores_the_rect_layout_wrote() {
    let mut tree = StyleTree::new();
    let id = tree.insert(None, Style::new()).unwrap();
    assert_eq!(tree.node(id).and_then(|n| n.rect), None);

    tree.set_rect(id, CellRect::new(1, 2, 3, 4)).unwrap();
    assert_eq!(tree.node(id).and_then(|n| n.rect), Some(CellRect::new(1, 2, 3, 4)));

    tree.remove(id).unwrap();
    assert_eq!(tree.set_rect(id, CellRect::default()), Err(TreeError::StaleNode(id)));
}
