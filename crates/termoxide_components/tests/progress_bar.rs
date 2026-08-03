use ratatui::{layout::Rect, style::Style};
use termoxide_components::progress_bar;
use termoxide_rendering::{
    builder::NodeBuilder,
    view_node::{ViewContent, ViewNode},
};

#[test]
fn progress_bar_is_a_raw_component_with_clamped_progress() {
    let node: ViewNode = progress_bar(120, 100, "Complete")
        .area(Rect::new(0, 0, 20, 1))
        .style(Style::default())
        .build();

    assert!(matches!(node.content, ViewContent::Raw(_)));
    assert_eq!(node.area, Rect::new(0, 0, 20, 1));
}
