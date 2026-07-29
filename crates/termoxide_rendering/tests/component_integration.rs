// //! Integration tests for the Component trait.
// //!
// //! Exercises:
// //! - simple render contract
// //! - props as plain Rust struct
// //! - basic child node construction

// use ratatui::layout::Rect;
// use ratatui::style::Style;

// use termoxide_rendering::component::Component;
// use termoxide_rendering::view_node::{ViewContent, ViewNode};

// #[derive(Debug)]
// struct HelloProps {
//     greeting: String,
//     items: Vec<String>,
// }

// struct HelloWorld {
//     props: HelloProps,
// }

// impl Component for HelloWorld {
//     fn render(&self) -> ViewNode {
//         let mut children = Vec::with_capacity(self.props.items.len() + 1);

//         children.push(ViewNode::text(
//             Rect::new(0, 0, 20, 1),
//             self.props.greeting.clone(),
//             Style::default(),
//         ));

//         for (index, item) in self.props.items.iter().enumerate() {
//             children.push(ViewNode::text(
//                 Rect::new(0, (index + 1) as u16, 20, 1),
//                 item.clone(),
//                 Style::default(),
//             ));
//         }

//         let height = (self.props.items.len() + 1) as u16;
//         ViewNode::container(Rect::new(0, 0, 20, height), children)
//     }
// }

// #[test]
// fn hello_world_component_renders_text_and_children() {
//     let component = HelloWorld {
//         props: HelloProps {
//             greeting: "Hello, TermOxide!".to_string(),
//             items: vec!["child-1".to_string(), "child-2".to_string()],
//         },
//     };

//     let node = component.render();

//     assert!(matches!(node.content, ViewContent::Container));
//     assert_eq!(node.children.len(), 3);

//     assert!(matches!(
//         &node.children[0].content,
//         ViewContent::Text { text, .. } if text == "Hello, TermOxide!"
//     ));
//     assert!(matches!(
//         &node.children[1].content,
//         ViewContent::Text { text, .. } if text == "child-1"
//     ));
//     assert!(matches!(
//         &node.children[2].content,
//         ViewContent::Text { text, .. } if text == "child-2"
//     ));
// }
