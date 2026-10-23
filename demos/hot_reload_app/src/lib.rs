//! A counter that ticks, to try hot reload on: built natively for the
//! executable, and as a wasm component for the guest.

use ratatui::{layout::Rect, style::Style};
use serde::{Deserialize, Serialize};
use termoxide::App;
use termoxide_event::event::{Event, KeyCode};
use termoxide_reactive::Signal;
use termoxide_rendering::{
    builder::{Container, NodeBuilder, el, text},
    view_node::ViewNode,
};

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct Counter {
    ticks: Signal<u64>,
}

impl Default for Counter {
    fn default() -> Self { Self { ticks: Signal::new(0) } }
}

impl App for Counter {
    fn track_view(&self) { let _ = self.ticks.get(); }

    fn on_tick(&self) { self.ticks.update(|ticks| *ticks += 1); }

    fn handle_event(&self, event: Event) -> bool {
        let quit = matches!(event, Event::KeyPress(key) if key.code == KeyCode::Char('q'));
        if quit {
            log::info!("quitting after {} ticks", self.ticks.get_untracked());
        }
        quit
    }

    fn build_view(&self, viewport: Rect) -> ViewNode {
        let line = text(format!("ticks: {} (q quits)", self.ticks.get_untracked()))
            .area(Rect::new(viewport.x, viewport.y, viewport.width, 1))
            .style(Style::default())
            .build();

        el(Container).area(viewport).children(vec![line]).build()
    }
}

termoxide_hot_reload::wasm_app!(Counter::default());
