use ratatui::{
    layout::Rect,
    widgets::{Gauge, Widget},
};
use crate::RsxComponent;
use termoxide_rendering::{
    builder::{CommonFields, NodeBuilder},
    view_node::ViewNode,
};

#[derive(RsxComponent)]
#[rsx(component = ProgressBar, builder = ProgressBarBuilder, constructor = progress_bar)]
pub struct ProgressBarProps {
    pub value: u16,
    pub max: u16,
    pub label: String,
}

pub struct ProgressBarBuilder {
    common: CommonFields,
    value: u16,
    max: u16,
    label: String,
    area: Rect,
}

impl ProgressBarBuilder {
    fn new(value: u16, max: u16, label: impl Into<String>) -> Self {
        Self {
            common: CommonFields::new(),
            value,
            max,
            label: label.into(),
            area: Rect::default(),
        }
    }

    pub fn area(mut self, area: Rect) -> Self {
        self.area = area;
        self
    }
}

impl NodeBuilder for ProgressBarBuilder {
    fn common(&mut self) -> &mut CommonFields {
        &mut self.common
    }

    fn build(self) -> ViewNode {
        let ratio = if self.max == 0 {
            0.0
        } else {
            f64::from(self.value.min(self.max)) / f64::from(self.max)
        };
        let style = self.common.style;
        let label = self.label;
        let area = self.area;
        let mut node = ViewNode::raw(area, move |buffer, area| {
            Gauge::default()
                .ratio(ratio)
                .label(label.clone())
                .gauge_style(style)
                .render(area, buffer);
        });
        node.id = self.common.id;
        node
    }
}

pub fn progress_bar(value: u16, max: u16, label: impl Into<String>) -> ProgressBarBuilder {
    ProgressBarBuilder::new(value, max, label)
}
