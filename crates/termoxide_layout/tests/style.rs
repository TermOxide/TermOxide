use termoxide_layout::style::{
    BorderSides,
    BorderStyle,
    CLASS,
    Color,
    Dirty,
    PropClass,
    PropId,
    Property,
    Sem,
    Style,
    Unit,
};

const BUTTON: Style = Style::new().bg(Color::Semantic(Sem::Surface)).bold(true).pad_x(2);
const HOVER: Style = Style::new().bg(Color::Semantic(Sem::Accent));
const BUTTON_HOVER: Style = BUTTON.merge(HOVER);

#[test]
fn const_merge_takes_the_right_side_where_it_is_set() {
    assert_eq!(BUTTON_HOVER.bg, Property::Value(Color::Semantic(Sem::Accent)));
    assert_eq!(BUTTON_HOVER.bold, Property::Value(true));
    assert_eq!(BUTTON_HOVER.padding_left, Property::Value(Unit::Cells(2)));
}

#[test]
fn merge_with_unset_keeps_the_left_side() {
    assert_eq!(BUTTON.merge(Style::new()), BUTTON);
    assert_eq!(Style::new().merge(BUTTON), BUTTON);
}

#[test]
fn inherit_overrides_a_value_in_merge() {
    let inherit = Style { fg: Property::Inherit, ..Style::new() };
    let merged = Style::new().fg(Color::Base16(1)).merge(inherit);
    assert_eq!(merged.fg, Property::Inherit);
}

#[test]
fn class_table_covers_every_property_in_order() {
    assert_eq!(CLASS.len(), PropId::ALL.len());
    for (index, id) in PropId::ALL.iter().enumerate() {
        assert_eq!(*id as usize, index);
        assert_eq!(id.class(), CLASS[index]);
    }
    assert_eq!(PropId::Fg.class(), PropClass::Paint);
    assert_eq!(PropId::TextWrap.class(), PropClass::Layout);
    assert_eq!(PropId::InsetLeft.class(), PropClass::Layout);
    assert_eq!(PropId::PaddingLeft.class(), PropClass::Both);
    assert_eq!(PropId::BorderSides.class(), PropClass::Both);
}

#[test]
fn dirty_ors_the_classes_of_changed_properties() {
    let base = Style::new().fg(Color::Base16(2)).width(Unit::Cells(10));

    assert_eq!(base.dirty(&base), Dirty { paint: false, layout: false });
    assert_eq!(base.dirty(&base.fg(Color::Base16(3))), Dirty { paint: true, layout: false });
    assert_eq!(base.dirty(&base.width(Unit::Cells(11))), Dirty { paint: false, layout: true });
    assert_eq!(base.dirty(&base.border_sides(BorderSides::TOP)), Dirty {
        paint: true,
        layout: true
    });
    assert_eq!(base.dirty(&base.fg(Color::Base16(3)).flex_grow(1.0)), Dirty {
        paint: true,
        layout: true
    });
}

#[test]
fn setters_clamp_negative_lengths_to_zero() {
    let s = Style::new()
        .width(Unit::Cells(-4))
        .max_height(Unit::Percent(-10.0))
        .flex_basis(Unit::Fr(-1.0))
        .gap(Unit::Cells(-1))
        .padding(-2)
        .flex_grow(-3.0)
        .flex_shrink(f32::NAN);

    assert_eq!(s.width, Property::Value(Unit::Cells(0)));
    assert_eq!(s.max_height, Property::Value(Unit::Percent(0.0)));
    assert_eq!(s.flex_basis, Property::Value(Unit::Fr(0.0)));
    assert_eq!(s.gap, Property::Value(Unit::Cells(0)));
    assert_eq!(s.padding_top, Property::Value(Unit::Cells(0)));
    assert_eq!(s.flex_grow, Property::Value(0.0));
    assert_eq!(s.flex_shrink, Property::Value(0.0));
}

#[test]
fn setters_keep_negative_insets() {
    assert_eq!(
        Style::new().inset_left(Unit::Cells(-3)).inset_left,
        Property::Value(Unit::Cells(-3))
    );
}

#[test]
fn padding_shorthands_set_their_sides_only() {
    let x = Style::new().pad_x(2);
    assert_eq!(
        (x.padding_left, x.padding_right),
        (Property::Value(Unit::Cells(2)), Property::Value(Unit::Cells(2)))
    );
    assert_eq!((x.padding_top, x.padding_bottom), (Property::Unset, Property::Unset));

    let y = Style::new().pad_y(1);
    assert_eq!(
        (y.padding_top, y.padding_bottom),
        (Property::Value(Unit::Cells(1)), Property::Value(Unit::Cells(1)))
    );
    assert_eq!((y.padding_left, y.padding_right), (Property::Unset, Property::Unset));

    assert_eq!(Style::new().padding(3), Style::new().pad_x(3).pad_y(3));
}

#[test]
fn border_sides_combine_and_report_drawn_sides() {
    let top_bottom = BorderSides::TOP.union(BorderSides::BOTTOM);
    assert!(top_bottom.contains(BorderSides::TOP));
    assert!(top_bottom.contains(BorderSides::BOTTOM));
    assert!(!top_bottom.contains(BorderSides::LEFT));
    assert_eq!(
        BorderSides::TOP
            .union(BorderSides::RIGHT)
            .union(top_bottom)
            .union(BorderSides::LEFT),
        BorderSides::ALL
    );

    let boxed = Style::new().border_style(BorderStyle::Rounded).border_sides(top_bottom);
    assert_eq!(boxed.border_style, Property::Value(BorderStyle::Rounded));
}

#[test]
fn property_value_ignores_unset_and_inherit() {
    assert_eq!(Property::Value(4).value(), Some(4));
    assert_eq!(Property::<i32>::Inherit.value(), None);
    assert_eq!(Property::<i32>::Unset.value(), None);
}

#[test]
fn default_style_is_all_unset() {
    assert_eq!(Style::default(), Style::new());
    assert_eq!(Style::new().dirty(&Style::default()), Dirty::default());
}
