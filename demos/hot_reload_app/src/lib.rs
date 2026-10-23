//! The mouse demo of `external_app_mouse` with hot reload: built natively for
//! the executable, and as a wasm component for the guest.

use ratatui::{
    layout::Rect,
    style::{Color, Style},
};
use serde::{Deserialize, Serialize};
use termoxide::App;
use termoxide_event::event::{Event, KeyCode, KeyModifiers};
use termoxide_reactive::Signal;
use termoxide_rendering::{
    builder::{Container, NodeBuilder, el, text},
    view_node::ViewNode,
};

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct AppState {
    count: Signal<u32>,
    ticks: Signal<u64>,
    last_key: Signal<String>,
    last_mouse: Signal<String>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            count: Signal::new(0),
            ticks: Signal::new(0),
            last_key: Signal::new(String::from("waiting for input")),
            last_mouse: Signal::new(String::from("waiting for input")),
        }
    }
}

impl AppState {
    fn line(viewport: Rect, row_offset: u16, content: String, style: Style) -> Option<ViewNode> {
        if row_offset >= viewport.height {
            return None;
        }

        Some(
            text(content)
                .area(Rect::new(viewport.x, viewport.y.saturating_add(row_offset), viewport.width, 1))
                .style(style)
                .build(),
        )
    }
}

impl App for AppState {
    fn track_view(&self) {
        let _ = self.count.get();
        let _ = self.ticks.get();
        let _ = self.last_key.get();
        let _ = self.last_mouse.get();
    }

    fn on_tick(&self) { self.ticks.update(|ticks| *ticks += 1); }

    fn handle_event(&self, event: Event) -> bool {
        match event {
            Event::ChannelReady => false,
            Event::KeyPress(key) => {
                self.count.update(|count| *count += 1);
                self.last_key.set(format!("key {}+{:?}", key.modifiers, key.code));
                let quit = key.code == KeyCode::Char('q')
                    || (key.code == KeyCode::Char('c') && key.modifiers == KeyModifiers::CONTROL);
                if quit {
                    log::info!("quitting after {} key presses", self.count.get_untracked());
                }
                quit
            },
            Event::Mouse(mouse) => {
                self.last_mouse.set(format!(
                    "{:?} at {}:{} with {}",
                    mouse.kind, mouse.column, mouse.row, mouse.modifiers
                ));
                false
            },
        }
    }

    fn mouse_capture(&self) -> bool { true }

    fn build_view(&self, viewport: Rect) -> ViewNode {
        let children: Vec<ViewNode> = [
            Self::line(
                viewport,
                0,
                "hot-reloaded termoxide app".to_string(),
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
                format!("last mouse event: {}", self.last_mouse.get_untracked()),
                Style::default().fg(Color::Magenta),
            ),
            Self::line(
                viewport,
                3,
                "Controls: any key counts, the mouse is reported, q or Ctrl-C quits".to_string(),
                Style::default().fg(Color::Green),
            ),
        ]
        .into_iter()
        .flatten()
        .collect();

        el(Container).area(viewport).children(children).build()
    }
}

termoxide_hot_reload::wasm_app!(AppState::default());
