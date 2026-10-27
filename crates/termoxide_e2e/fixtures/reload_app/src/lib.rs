//! Counts presses of `a`; `q` quits.

use ratatui::{layout::Rect, style::Style};
use serde::{Deserialize, Serialize};
use termoxide::App;
use termoxide_event::event::{Event, KeyCode};
use termoxide_reactive::Signal;
use termoxide_rendering::view_node::ViewNode;

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct Presses {
    count: Signal<u32>,
}

impl Default for Presses {
    fn default() -> Self { Self { count: Signal::new(0) } }
}

impl App for Presses {
    fn track_view(&self) { let _ = self.count.get(); }

    fn on_tick(&self) {}

    fn handle_event(&self, event: Event) -> bool {
        match event {
            Event::KeyPress(key) if key.code == KeyCode::Char('a') => {
                self.count.update(|count| *count += 1);
                false
            },
            Event::KeyPress(key) => key.code == KeyCode::Char('q'),
            _ => false,
        }
    }

    fn build_view(&self, viewport: Rect) -> ViewNode {
        let line = Rect::new(viewport.x, viewport.y, viewport.width, 1);
        ViewNode::text(line, format!("presses: {}", self.count.get_untracked()), Style::default())
    }

    fn mouse_capture(&self) -> bool { false }
}

termoxide_hot_reload::wasm_app!(Presses::default());
