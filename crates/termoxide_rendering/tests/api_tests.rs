use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use termoxide_rendering::event_router::EventRouter;
use termoxide_rendering::builder::{button, Msg};
use termoxide_rendering::view::{el, text, Container, StyleExt};
use termoxide_rendering::view_node::ViewContent;
use termoxide_rendering::view_node::ViewNode;


#[test]
fn builder_creates_empty_container() {
    let style = Style::default().area(Rect::new(0, 0, 10, 2));
    let root = el(Container).style(style).build();

    assert!(matches!(root.content, ViewContent::Container));
    assert_eq!(root.children.len(), 0);
    assert_eq!(root.area, Rect::new(0, 0, 10, 2));
}

#[test]
fn builder_creates_single_text_node() {
    let root = text("Single Node").build();

    assert!(matches!(
        root.content,
        ViewContent::Text { text, .. } if text == "Single Node"
    ));
    assert_eq!(root.children.len(), 0);
    assert_eq!(root.area, Rect::new(0, 0, 0, 0));
}

#[test]
fn builder_add_single_child_to_container() {
    let style = Style::default().area(Rect::new(0, 0, 20, 4));
    let root = el(Container).style(style).children([text("Child Node")]).build();

    assert!(matches!(root.content, ViewContent::Container));
    assert_eq!(root.children.len(), 1);
    assert_eq!(root.area, Rect::new(0, 0, 20, 4));

    assert!(matches!(
        &root.children[0].content,
        ViewContent::Text { text, .. } if text == "Child Node"
    ));
}

#[test]
fn builder_add_multiple_children_to_container() {
    let style = Style::default().area(Rect::new(0, 0, 25, 5));
    let root = el(Container).style(style).children([text("Child 1"), text("Child 2"), text("Child 3")]).build();

    assert!(matches!(root.content, ViewContent::Container));
    assert_eq!(root.children.len(), 3);
    assert_eq!(root.area, Rect::new(0, 0, 25, 5));

    assert!(matches!(
        &root.children[0].content,
        ViewContent::Text { text, .. } if text == "Child 1"
    ));
    assert!(matches!(
        &root.children[1].content,
        ViewContent::Text { text, .. } if text == "Child 2"
    ));
    assert!(matches!(
        &root.children[2].content,
        ViewContent::Text { text, .. } if text == "Child 3"
    ));
}

#[test]
fn builder_creates_nested_containers() {
    let style = Style::default().area(Rect::new(0, 0, 30, 10));
    let root = el(Container)
        .style(style)
        .children([
            el(Container)
                .style(Style::default().area(Rect::new(0, 0, 15, 5)))
                .children([text("Nested 1")])
                .build(),
            el(Container)
                .style(Style::default().area(Rect::new(15, 0, 15, 5)))
                .children([text("Nested 2")])
                .build(),
        ])
        .build();

    assert!(matches!(root.content, ViewContent::Container));
    assert_eq!(root.children.len(), 2);
    assert_eq!(root.area, Rect::new(0, 0, 30, 10));
    assert!(matches!(root.children[0].content, ViewContent::Container));
    assert!(matches!(root.children[1].content, ViewContent::Container));
    assert_eq!(root.children[0].area, Rect::new(0, 0, 15, 5));
    assert_eq!(root.children[1].area, Rect::new(15, 0, 15, 5));
    assert!(matches!(
        &root.children[0].children[0].content,
        ViewContent::Text { text, .. } if text == "Nested 1"
    ));
    assert!(matches!(
        &root.children[1].children[0].content,
        ViewContent::Text { text, .. } if text == "Nested 2"
    ));
}

#[test]
fn builder_supports_styled_text() {
    let style = Style::default().fg(Color::Red).bg(Color::Black).bold();
    let root = text("Styled Text")
        .style(style)
        .build();

    assert!(matches!(
        root.content,
        ViewContent::Text { text, style: _ } if text == "Styled Text"
    ));
    assert_eq!(root.children.len(), 0);
    assert_eq!(root.area, Rect::new(0, 0, 0, 0));
}

#[test]
fn builder_support_default_style_text() {
    let root = text("Default Style").build();

    assert!(matches!(
        root.content,
        ViewContent::Text { text, style } if text == "Default Style" && *style == Style::default()
    ));
    assert_eq!(root.children.len(), 0);
    assert_eq!(root.area, Rect::new(0, 0, 0, 0));
}

#[test]
fn builder_support_styled_container() {
    let style = Style::default().fg(Color::Blue).bg(Color::White).italic().area(Rect::new(0, 0, 20, 5));
    let root = el(Container)
        .style(style)
        .children([text("Styled Container")])
        .build();

    assert!(matches!(root.content, ViewContent::Container { style: _ }));
    assert_eq!(root.children.len(), 1);
    assert_eq!(root.area, Rect::new(0, 0, 20, 5));
    assert!(matches!(
        &root.children[0].content,
        ViewContent::Text { text, .. } if text == "Styled Container"
    ));
}

#[test]
fn builder_support_default_style_container() {
    let style = Style::default().area(Rect::new(0, 0, 20, 5));
    let root = el(Container)
        .style(style)
        .children([text("Default Style Container")])
        .build();

    assert!(matches!(root.content, ViewContent::Container { style } if *style == Style::default()));
    assert_eq!(root.children.len(), 1);
    assert_eq!(root.area, Rect::new(0, 0, 20, 5));
    assert!(matches!(
        &root.children[0].content,
        ViewContent::Text { text, style } if text == "Default Style Container" && *style == Style::default()
    ));
}

#[test]
fn builder_supports_nested_styled_containers() {
    let style1 = Style::default().fg(Color::Green).bg(Color::Black).bold().area(Rect::new(0, 0, 30, 10));
    let style2 = Style::default().fg(Color::Yellow).bg(Color::Blue).italic().area(Rect::new(0, 0, 15, 5));
    let root = el(Container)
        .style(style1)
        .children([
            el(Container)
                .style(style2)
                .children([text("Nested Styled 1")])
                .build(),
            el(Container)
                .style(Style::default().fg(Color::Magenta).bg(Color::Cyan).underline().area(Rect::new(15, 0, 15, 5)))
                .children([text("Nested Styled 2")])
                .build(),
        ])
        .build();

    assert!(matches!(root.content, ViewContent::Container { style: _ }));
    assert_eq!(root.children.len(), 2);
    assert_eq!(root.area, Rect::new(0, 0, 30, 10));
    assert!(matches!(root.children[0].content, ViewContent::Container { style: _ }));
    assert!(matches!(root.children[1].content, ViewContent::Container { style: _ }));
    assert_eq!(root.children[0].area, Rect::new(0, 0, 15, 5));
    assert_eq!(root.children[1].area, Rect::new(15, 0, 15, 5));
    assert!(matches!(
        &root.children[0].children[0].content,
        ViewContent::Text { text, style: _ } if text == "Nested Styled 1"
    ));
    assert!(matches!(
        &root.children[1].children[0].content,
        ViewContent::Text { text, style: _ } if text == "Nested Styled 2"
    ));
}

#[test]
fn builder_sets_id() {
    let style = Style::default().area(Rect::new(0, 0, 20, 5));
    let root = el(Container)
        .style(style)
        .id("root_container")
        .children([text("Child Node")])
        .build();

    assert_eq!(root.id.as_deref(), Some("root_container"));
    assert!(matches!(root.content, ViewContent::Container));
    assert_eq!(root.children.len(), 1);
    assert_eq!(root.area, Rect::new(0, 0, 20, 5));
}

#[test]
fn builder_sets_id_on_nested_container() {
    let style1 = Style::default().area(Rect::new(0, 0, 30, 10));
    let style2 = Style::default().area(Rect::new(0, 0, 15, 5));
    let root = el(Container)
        .style(style1)
        .children([
            el(Container)
                .style(style2)
                .id("nested_container_1")
                .children([text("Nested 1")])
                .build(),
            el(Container)
                .style(Style::default().area(Rect::new(15, 0, 15, 5)))
                .id("nested_container_2")
                .children([text("Nested 2")])
                .build(),
        ])
        .build();

    assert!(matches!(root.content, ViewContent::Container));
    assert_eq!(root.children.len(), 2);
    assert_eq!(root.area, Rect::new(0, 0, 30, 10));
    assert_eq!(root.children[0].id.as_deref(), Some("nested_container_1"));
    assert_eq!(root.children[1].id.as_deref(), Some("nested_container_2"));
    assert!(matches!(
        &root.children[0].children[0].content,
        ViewContent::Text { text, .. } if text == "Nested 1"
    ));
    assert!(matches!(
        &root.children[1].children[0].content,
        ViewContent::Text { text, .. } if text == "Nested 2"
    ));
}

#[test]
fn builds_add_class() {
    let style = Style::default().area(Rect::new(0, 0, 20, 5));
    let root = el(Container)
        .style(style)
        .class("root_class")
        .children([text("Child Node")])
        .build();

    assert!(root.classes.contains("root_class"));
    assert!(matches!(root.content, ViewContent::Container));
    assert_eq!(root.children.len(), 1);
    assert_eq!(root.area, Rect::new(0, 0, 20, 5));
}

#[test]
fn builds_add_key() {
    let root = el(Container)
        .style(Style::default().area(Rect::new(0, 0, 20, 5)))
        .key("root_key")
        .children([text("Child Node")])
        .build();

    assert_eq!(root.key.as_deref(), Some("root_key"));
    assert!(matches!(root.content, ViewContent::Container));
    assert_eq!(root.children.len(), 1);
    assert_eq!(root.area, Rect::new(0, 0, 20, 5));
}

#[test]
fn builds_handle_events() {
    let root = el(Container)
        .style(Style::default().area(Rect::new(0, 0, 20, 5)))
        .on_event(|event| {
            println!("Event received: {:?}", event);
        })
        .children([text("Child Node")])
        .build();

    assert!(root.event_handler.is_some());
    assert!(matches!(root.content, ViewContent::Container));
    assert_eq!(root.children.len(), 1);
    assert_eq!(root.area, Rect::new(0, 0, 20, 5));
}

#[test]
fn builds_button_with_click_event() {
    let root = button("Run", Msg::Run);
    assert!(matches!(root.content, ViewContent::Button { .. }));
}

#[test]
fn builds_button_with_custom_event() {
    let root = button("Custom", Msg::CustomEvent);
    assert!(matches!(root.content, ViewContent::Button { .. }));
}

#[test]
fn builds_dynamic_lists() {
    let items = vec!["Item 1", "Item 2", "Item 3"];
    let style = Style::default().area(Rect::new(0, 0, 30, 10));
    let root = el(Container)
        .style(style)
        .children(
            items
                .iter()
                .map(|&item| text(item).build())
                .collect::<Vec<_>>(),
        )
        .build();

    assert!(matches!(root.content, ViewContent::Container));
    assert_eq!(root.children.len(), items.len());
    for (i, child) in root.children.iter().enumerate() {
        assert!(matches!(
            &child.content,
            ViewContent::Text { text, .. } if text == items[i]
        ));
    }
}

#[test]
fn builds_nested_dynamic_lists() {
    let outer_items = vec!["Outer 1", "Outer 2"];
    let inner_items = vec!["Inner A", "Inner B"];
    let style = Style::default().area(Rect::new(0, 0, 40, 15));
    let root = el(Container)
        .style(style)
        .children(
            outer_items
                .iter()
                .map(|&outer| {
                    el(Container)
                        .style(Style::default().area(Rect::new(0, 0, 20, 5)))
                        .children(
                            inner_items
                                .iter()
                                .map(|&inner| text(format!("{} - {}", outer, inner)).build())
                                .collect::<Vec<_>>(),
                        )
                        .build()
                })
                .collect::<Vec<_>>(),
        )
        .build();

    assert!(matches!(root.content, ViewContent::Container));
    assert_eq!(root.children.len(), outer_items.len());
    for (i, outer_child) in root.children.iter().enumerate() {
        assert!(matches!(outer_child.content, ViewContent::Container));
        assert_eq!(outer_child.children.len(), inner_items.len());
        for (j, inner_child) in outer_child.children.iter().enumerate() {
            assert!(matches!(
                &inner_child.content,
                ViewContent::Text { text, .. } if text == format!("{} - {}", outer_items[i], inner_items[j])
            ));
        }
    }
}
