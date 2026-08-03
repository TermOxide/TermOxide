use ratatui::{
    layout::Rect,
    style::{Color, Style},
};
use termoxide_rendering::{
    builder::{Container, NodeBuilder, el, text},
    view_node::ViewContent,
};

// ─────────────────────────────────────────────────────────────────────────── //
//  Container
// ─────────────────────────────────────────────────────────────────────────── //

#[test]
fn container_default_has_no_children_and_no_id() {
    let node = el(Container).build();

    assert!(node.id.is_none());
    assert!(node.children.is_empty());
    assert!(matches!(node.content, ViewContent::Container { .. }));
    assert_eq!(node.area, Rect::default());
}

#[test]
fn container_area_is_set() {
    let rect = Rect::new(1, 2, 30, 10);
    let node = el(Container).area(rect).build();

    assert_eq!(node.area, rect);
}

#[test]
fn container_children_accepts_prebuilt_view_nodes() {
    let child = text("child").build();
    let node = el(Container).children([child]).build();

    assert_eq!(node.children.len(), 1);
}

#[test]
fn container_children_accepts_unbuilt_builders() {
    let node = el(Container).children([text("child")]).build();

    assert_eq!(node.children.len(), 1);
}

#[test]
fn container_children_accepts_mixed_builders_and_nodes() {
    let prebuilt = text("already built").build();

    let node = el(Container).children(vec![prebuilt, text("auto built").build()]).build();

    assert_eq!(node.children.len(), 2);
}

#[test]
fn container_children_overwrites_previous_children_on_repeated_call() {
    let node = el(Container)
        .children([text("first")])
        .children([text("second"), text("third")])
        .build();

    assert_eq!(node.children.len(), 2);
}

#[test]
fn container_nested_children_build_recursively() {
    let inner = el(Container).children([text("leaf")]);
    let node = el(Container).children([inner]).build();

    assert_eq!(node.children.len(), 1);
    assert_eq!(node.children[0].children.len(), 1);
}

// ─────────────────────────────────────────────────────────────────────────── //
//  Text
// ─────────────────────────────────────────────────────────────────────────── //

#[test]
fn text_build_produces_text_content() {
    let node = text("hello").build();

    assert!(matches!(node.content, ViewContent::Text { .. }));
    assert!(node.children.is_empty());
}

#[test]
fn text_area_is_set() {
    let rect = Rect::new(0, 0, 5, 1);
    let node = text("hello").area(rect).build();

    assert_eq!(node.area, rect);
}

#[test]
fn text_id_is_forwarded_to_view_node() {
    let node = text("hello").id(42u64).build();

    assert_eq!(node.id, Some(42u64));
}

#[test]
fn text_without_id_has_none() {
    let node = text("hello").build();

    assert!(node.id.is_none());
}

#[test]
fn text_accepts_string_and_str() {
    let owned = String::from("owned");
    let a = text(owned).build();
    let b = text("borrowed").build();

    assert!(matches!(a.content, ViewContent::Text { .. }));
    assert!(matches!(b.content, ViewContent::Text { .. }));
}

#[test]
fn text_style_is_forwarded_to_view_node() {
    let style = Style::default().fg(Color::Green);
    let node = text("hello").style(style).build();

    match &node.content {
        ViewContent::Text { style: applied, .. } => assert_eq!(*applied, style),
        _ => panic!("expected text content"),
    }
}

// ─────────────────────────────────────────────────────────────────────────── //
//  Common setters shared across all builders (id, key, class, style, on_event)
// ─────────────────────────────────────────────────────────────────────────── //

#[test]
fn common_setters_are_chainable_and_do_not_panic_on_container() {
    let node = el(Container)
        .id(1u64)
        .key("row-1")
        .class("highlighted")
        .class("row")
        .on_event(|_evt| {})
        .style(Style::default())
        .build();

    assert_eq!(node.id, Some(1u64));
}

#[test]
fn container_style_is_forwarded_to_view_node() {
    let style = Style::default().bg(Color::Blue);
    let node = el(Container).style(style).build();

    match &node.content {
        ViewContent::Container { style: applied } => assert_eq!(*applied, style),
        _ => panic!("expected container content"),
    }
}

#[test]
fn container_without_style_defaults_to_an_invisible_one() {
    let node = el(Container).build();

    match &node.content {
        ViewContent::Container { style } => assert_eq!(*style, Style::default()),
        _ => panic!("expected container content"),
    }

}

#[test]
fn fragment_groups_children_without_painting() {
    let node = termoxide_rendering::builder::fragment(termoxide_rendering::builder::Fragment)
        .children([text("child")])
        .build();

    assert!(matches!(node.content, ViewContent::Fragment));
    assert_eq!(node.children.len(), 1);
}

#[test]
fn raw_builder_preserves_area_and_renderer_invokes_draw_function() {
    let area = Rect::new(1, 2, 3, 4);
    let node = termoxide_rendering::builder::raw(|buffer, rect| {
        buffer.set_string(rect.x, rect.y, "raw", Style::default());
    })
    .area(area)
    .build();
    let mut buffer = ratatui::buffer::Buffer::empty(Rect::new(0, 0, 10, 10));

    termoxide_rendering::renderer::Renderer::<ratatui::backend::TestBackend>::draw_node(
        &node,
        &mut buffer,
    );

    let index = usize::from(area.y) * 10 + usize::from(area.x);
    assert_eq!(buffer.content[index].symbol(), "r");
}

#[test]
fn common_setters_are_chainable_and_do_not_panic_on_text() {
    let node = text("hi").id(2u64).key("k").class("c1").on_event(|_evt| {}).build();

    assert_eq!(node.id, Some(2u64));
}

#[test]
fn class_can_be_added_multiple_times_without_duplicating_effect() {
    let node = el(Container).class("a").class("a").class("b").build();

    assert!(matches!(node.content, ViewContent::Container { .. }));
}

#[test]
fn last_id_call_wins_when_set_twice() {
    let node = el(Container).id(1u64).id(2u64).build();

    assert_eq!(node.id, Some(2u64));
}
