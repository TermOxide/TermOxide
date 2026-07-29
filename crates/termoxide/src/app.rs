use ratatui::{
    layout::Rect,
    style::{Color, Style},
};
use termoxide_event::event::{Event, KeyCode, KeyModifiers};
use termoxide_reactive::Signal;
use termoxide_rendering::{
    builder::{Container, NodeBuilder, el, text},
    view_node::ViewNode,
};

#[derive(Clone, Copy)]
pub(crate) struct AppState {
    count: Signal<u32>,
    ticks: Signal<u64>,
    last_key: Signal<String>,
}

impl AppState {
    pub(crate) fn new() -> Self {
        Self {
            count: Signal::new(0),
            ticks: Signal::new(0),
            last_key: Signal::new(String::from("waiting for input")),
        }
    }

    pub(crate) fn track_view(&self) {
        let _ = self.count.get();
        let _ = self.ticks.get();
        let _ = self.last_key.get();
    }

    pub(crate) fn on_tick(&self) {
        self.ticks.update(|ticks| *ticks += 1);
    }

    pub(crate) fn handle_event(&self, event: Event) -> bool {
        match event {
            Event::ChannelReady => {
                self.last_key.set(String::from("waiting for input"));
                false
            },
            Event::KeyPress(key) => {
                self.count.update(|count| *count += 1);
                self.last_key
                    .set(format!("key {:?}+{:?}", key.modifiers, key.code));
                key.code == KeyCode::Char('q')
                    || (key.code == KeyCode::Char('c')
                        && key.modifiers == KeyModifiers::CONTROL)
            },
        }
    }

    pub(crate) fn build_view(&self, viewport: Rect) -> ViewNode {
        let children: Vec<ViewNode> = [
            Self::line(
                viewport,
                0,
                "TermOxide demo".to_string(),
                Style::default().fg(Color::Cyan),
            ),
            Self::line(
                viewport,
                1,
                format!(
                    "ticks: {} | key presses: {} | last key: {}",
                    self.ticks.get_untracked(),
                    self.count.get_untracked(),
                    self.last_key.get_untracked(),
                ),
                Style::default().fg(Color::Yellow),
            ),
            Self::line(
                viewport,
                2,
                "Controls: any key counts, q or Ctrl-C quits".to_string(),
                Style::default().fg(Color::Green),
            ),
            Self::line(
                viewport,
                3,
                format!("waiting state: {}", self.last_key.get_untracked()),
                Style::default().fg(Color::DarkGray),
            ),
        ]
        .into_iter()
        .flatten()
        .collect();

        el(Container).area(viewport).children(children).build()
    }

    fn line(
        viewport: Rect,
        row_offset: u16,
        content: String,
        style: Style,
    ) -> Option<ViewNode> {
        if row_offset >= viewport.height {
            return None;
        }

        Some(
            text(content)
                .area(Rect::new(
                    viewport.x,
                    viewport.y.saturating_add(row_offset),
                    viewport.width,
                    1,
                ))
                .style(style)
                .build(),
        )
    }
}
