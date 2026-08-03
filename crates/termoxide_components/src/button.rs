use ratatui::layout::Rect;
use crate::RsxComponent;
use termoxide_rendering::{
    builder::{CommonFields, NodeBuilder},
    view_node::ViewNode,
};

#[derive(RsxComponent)]
#[rsx(component = Button, builder = ButtonBuilder, constructor = button)]
pub struct ButtonProps {
    pub label: String,
    pub msg: Msg,
}

/// Message associated with a button activation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Msg {
    Run,
    CustomEvent,
}

/// Builder for a text button component.
pub struct ButtonBuilder {
    common: CommonFields,
    label: String,
    msg: Msg,
    area: Rect,
}

impl ButtonBuilder {
    fn new(label: impl Into<String>, msg: Msg) -> Self {
        Self { common: CommonFields::new(), label: label.into(), msg, area: Rect::default() }
    }

    pub fn area(mut self, area: Rect) -> Self {
        self.area = area;
        self
    }

    /// Return the activation message carried by this component.
    pub fn message(&self) -> &Msg {
        &self.msg
    }
}

impl NodeBuilder for ButtonBuilder {
    fn common(&mut self) -> &mut CommonFields {
        &mut self.common
    }

    fn build(self) -> ViewNode {
        let label = termoxide_rendering::builder::text(self.label)
            .area(self.area)
            .style(self.common.style)
            .build();
        let mut node = ViewNode::fragment(self.area, vec![label]);
        node.id = self.common.id;
        node
    }
}

/// Construct a button component.
pub fn button(label: impl Into<String>, msg: Msg) -> ButtonBuilder {
    ButtonBuilder::new(label, msg)
}
