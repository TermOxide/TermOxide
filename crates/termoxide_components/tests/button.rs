use ratatui::{layout::Rect, style::{Color, Style}};
use termoxide_components::{Msg, button};
use termoxide_rendering::{builder::NodeBuilder, view_node::ViewContent};

#[test]
fn button_is_a_component_built_from_runtime_primitives() {
    let area = Rect::new(2, 3, 12, 1);
    let node = button("Run", Msg::Run).area(area).build();

    assert!(matches!(node.content, ViewContent::Fragment));
    assert_eq!(node.area, area);
    assert_eq!(node.children.len(), 1);
    assert_eq!(node.children[0].area, area);
}

#[test]
fn button_keeps_message_and_style() {
    let button = button("Run", Msg::CustomEvent);
    assert_eq!(button.message(), &Msg::CustomEvent);

    let node = button.style(Style::default().fg(Color::Green)).build();
    match &node.children[0].content {
        ViewContent::Text { style, .. } => assert_eq!(style.fg, Some(Color::Green)),
        _ => panic!("button label must be text"),
    }
}
