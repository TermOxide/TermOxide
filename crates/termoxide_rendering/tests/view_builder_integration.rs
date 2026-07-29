//! Integration tests for the builder-style View API.

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use termoxide_rendering::view::{badge, button, el, link, text, text_with_style, Container, StyleExt};
use termoxide_rendering::view_node::{ComponentId, ViewContent};

#[test]
fn builder_creates_tree() {
    let root = el(Container)
        .area(Rect::new(0, 0, 20, 3))
        .children([text("Hello"), text("World")])
        .build();

    assert!(matches!(root.content, ViewContent::Container));
    assert_eq!(root.children.len(), 2);
    assert_eq!(root.area, Rect::new(0, 0, 20, 3));

    assert!(matches!(
        &root.children[0].content,
        ViewContent::Text { text, .. } if text == "Hello"
    ));
    assert!(matches!(
        &root.children[1].content,
        ViewContent::Text { text, .. } if text == "World"
    ));
}

#[test]
fn builder_supports_styled_children() {
    let hello_style = Style::default().fg(Color::Cyan);
    let world_style = Style::default().fg(Color::Yellow);

    let root = el(Container)
        .children([
            text_with_style("Hello", hello_style),
            text_with_style("World", world_style),
        ])
        .build();

    assert!(matches!(
        &root.children[0].content,
        ViewContent::Text { text, style } if text == "Hello" && *style == hello_style
    ));
    assert!(matches!(
        &root.children[1].content,
        ViewContent::Text { text, style } if text == "World" && *style == world_style
    ));
}

#[test]
fn builder_complex_tree_with_components() {
    let header_style = Style::default().fg(Color::Green);

    let root = el(Container)
        .area(Rect::new(0, 0, 40, 6))
        .children([
            text_with_style("Header", header_style),
            el(Container)
                .children([link("Docs", 1), button("Run", 2), badge("Beta")])
                .build(),
            text("Footer"),
        ])
        .build();

    assert!(matches!(root.content, ViewContent::Container));
    assert_eq!(root.children.len(), 3);
    assert_eq!(root.area, Rect::new(0, 0, 40, 6));

    assert!(matches!(
        &root.children[0].content,
        ViewContent::Text { text, style } if text == "Header" && *style == header_style
    ));

    assert!(matches!(root.children[1].content, ViewContent::Container));
    assert_eq!(root.children[1].children.len(), 3);
    assert_eq!(root.children[1].children[0].id, Some(1));
    assert_eq!(root.children[1].children[1].id, Some(2));
    assert_eq!(root.children[1].children[2].id, None as Option<ComponentId>);

    assert!(matches!(
        &root.children[1].children[0].content,
        ViewContent::Text { text, .. } if text == "Docs"
    ));
    assert!(matches!(
        &root.children[1].children[1].content,
        ViewContent::Text { text, .. } if text == "Run"
    ));
    assert!(matches!(
        &root.children[1].children[2].content,
        ViewContent::Text { text, .. } if text == "Beta"
    ));

    assert!(matches!(
        &root.children[2].content,
        ViewContent::Text { text, .. } if text == "Footer"
    ));
}
