#![cfg(feature = "future")]

use taffy::style::{Dimension, Display, FlexDirection, LengthPercentage};
use termoxide_layout::{
    engine::LayoutEngine,
    style::{Display as UiDisplay, FlexDirection as UiFlexDirection, Style, Unit},
};

#[test]
fn from_ui_style_maps_display_sizes_and_padding() {
    let ui = Style::new()
        .display(UiDisplay::Flex)
        .flex_direction(UiFlexDirection::Row)
        .width(Unit::Cells(40))
        .height(Unit::Percent(100.0))
        .padding(1);

    let ts = LayoutEngine::from_ui_style(&ui);

    assert_eq!(ts.display, Display::Flex);
    assert_eq!(ts.flex_direction, FlexDirection::Row);
    assert_eq!(ts.size.width, Dimension::length(40.0));
    assert_eq!(ts.size.height, Dimension::percent(1.0));
    assert_eq!(ts.padding.left, LengthPercentage::length(1.0));
}

#[test]
fn from_ui_style_keeps_taffy_defaults_for_unset() {
    assert_eq!(LayoutEngine::from_ui_style(&Style::new()), taffy::Style::DEFAULT);
}
