#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
};
use serde::{Serialize, de::DeserializeOwned};
use termoxide_rendering::view_node::{ViewContent, ViewNode};

fn round_trip<T: Serialize + DeserializeOwned>(value: &T) -> T {
    postcard::from_bytes(&postcard::to_allocvec(value).unwrap()).unwrap()
}

#[test]
fn styles_round_trip_with_every_kind_of_color() {
    let colors = [
        Color::Reset,
        Color::Black,
        Color::LightMagenta,
        Color::Gray,
        Color::White,
        Color::Rgb(255, 127, 80),
        Color::Indexed(240),
    ];

    for color in colors {
        let style = Style::default()
            .fg(color)
            .bg(Color::Blue)
            .add_modifier(Modifier::BOLD)
            .remove_modifier(Modifier::ITALIC);
        let node = round_trip(&ViewNode::text(Rect::new(0, 0, 5, 1), "hi", style));

        let ViewContent::Text { text, style: back } = node.content else {
            panic!("expected text")
        };
        assert_eq!((text.as_str(), back), ("hi", style));
    }
}

#[test]
fn an_unset_color_stays_unset() {
    let node = round_trip(&ViewNode::container(Rect::new(0, 0, 1, 1), vec![]));

    let ViewContent::Container { style } = node.content else {
        panic!("expected a container")
    };
    assert_eq!(style, Style::default());
}

#[test]
fn a_tree_keeps_its_shape_ids_and_areas() {
    let root = ViewNode::styled_container(
        Rect::new(0, 0, 10, 3),
        vec![
            ViewNode::text(Rect::new(0, 0, 10, 1), "first", Style::default()).with_id(1),
            ViewNode::container(Rect::new(0, 1, 10, 2), vec![ViewNode::text(
                Rect::new(0, 1, 10, 1),
                "nested",
                Style::default(),
            )]),
        ],
        Style::default().bg(Color::Green),
    );

    let back = round_trip(&root);

    assert_eq!(back.area, root.area);
    assert!(matches!(back.content, ViewContent::Container { style } if style.bg == Some(Color::Green)));
    assert_eq!(back.children.len(), 2);
    assert_eq!(back.children[0].id, Some(1));
    assert_eq!(back.children[1].children[0].area, Rect::new(0, 1, 10, 1));
}
