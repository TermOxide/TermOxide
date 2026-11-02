use std::rc::Rc;

use termoxide_layout::style::{BorderSides, Color, Inherited, Property, Style, Unit, resolve};

fn child_of(parent: &(Style, Rc<Inherited>), own: &Style) -> (Style, Rc<Inherited>) {
    resolve(own, Some((&parent.0, &parent.1)))
}

#[test]
fn root_resolves_unset_and_inherit_paint_to_initial_values() {
    let (target, record) = resolve(&Style { fg: Property::Inherit, ..Style::new() }, None);
    assert_eq!(target.fg, Property::Unset);
    assert_eq!(*record, Inherited::default());
}

#[test]
fn node_overriding_nothing_shares_its_parent_record() {
    let panel = resolve(&Style::new().fg(Color::Base16(2)), None);
    let layout_only = child_of(&panel, &Style::new().width(Unit::Cells(4)));
    let explicit = child_of(&panel, &Style { fg: Property::Inherit, ..Style::new() });

    assert!(Rc::ptr_eq(&panel.1, &layout_only.1));
    assert!(Rc::ptr_eq(&panel.1, &explicit.1));
}

#[test]
fn overriding_node_allocates_a_record_over_its_parent() {
    let panel = resolve(&Style::new().fg(Color::Base16(2)).italic(true), None);
    let (target, record) = child_of(&panel, &Style::new().fg(Color::Base16(1)));

    assert!(!Rc::ptr_eq(&panel.1, &record));
    assert_eq!(record.fg, Some(Color::Base16(1)));
    assert_eq!(target.italic, Property::Value(true));
}

#[test]
fn paint_inherits_through_unset_and_inherit() {
    let panel = resolve(&Style::new().bg(Color::Base16(4)), None);
    let middle = child_of(&panel, &Style::new());
    let leaf = child_of(&middle, &Style { bg: Property::Inherit, ..Style::new() });
    assert_eq!(leaf.0.bg, Property::Value(Color::Base16(4)));
}

#[test]
fn layout_does_not_inherit_unless_asked() {
    let parent = resolve(&Style::new().width(Unit::Cells(10)).padding(2), None);
    let (unset, _) = child_of(&parent, &Style::new());
    let (asked, _) = child_of(&parent, &Style {
        width: Property::Inherit,
        padding_left: Property::Inherit,
        ..Style::new()
    });

    assert_eq!(unset.width, Property::Unset);
    assert_eq!(unset.padding_left, Property::Unset);
    assert_eq!(asked.width, Property::Value(Unit::Cells(10)));
    assert_eq!(asked.padding_left, Property::Value(Unit::Cells(2)));
}

#[test]
fn layout_inherit_reads_the_parent_resolved_value() {
    let root = resolve(&Style::new().border_sides(BorderSides::ALL), None);
    let middle = child_of(&root, &Style { border_sides: Property::Inherit, ..Style::new() });
    let leaf = child_of(&middle, &Style { border_sides: Property::Inherit, ..Style::new() });
    assert_eq!(leaf.0.border_sides, Property::Value(BorderSides::ALL));
}

#[test]
fn layout_inherit_at_a_root_is_unset() {
    let (target, _) = resolve(&Style { width: Property::Inherit, ..Style::new() }, None);
    assert_eq!(target.width, Property::Unset);
}
