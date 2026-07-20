//! Manual smoke test for the render loop wired to `termoxide_event`.
//!
//! Run with `cargo run -p termoxide_rendering --example render_loop` and check
//! the three things no automated test can cover:
//!
//! 1. **Modifiers travel end to end** — Ctrl-C and Ctrl-D quit, while a bare
//!    `c` or `d` only bumps the counter.
//! 2. **Resize still relayouts** — the stream carries no resize event, so the
//!    loop samples the viewport every frame instead. Resize the terminal while
//!    idle (without typing) and the reported size must follow.
//! 3. **The terminal is left clean** — on exit, no alternate screen and no
//!    residual raw mode. Type `blah` at the shell afterwards to confirm.

use std::io::stdout;

use ratatui::{Terminal, backend::CrosstermBackend, layout::Rect, style::Style};
use termoxide_event::{
    EventStream,
    event::{Event, KeyCode},
};
use termoxide_rendering::{
    event_router::EventRouter,
    render_loop::{App, RenderLoop},
    renderer::Renderer,
    view_node::{ComponentId, ViewNode},
};

struct Demo {
    presses: u32,
    last: String,
    viewport: Rect,
}

impl App for Demo {
    fn build_view(&mut self, viewport: Rect) -> ViewNode {
        self.viewport = viewport;
        let text = format!(
            "presses: {} | last: {} | size: {}x{} | q or Ctrl-C to quit",
            self.presses, self.last, viewport.width, viewport.height
        );
        ViewNode::text(viewport, text, Style::default())
    }

    fn handle_event(&mut self, _id: Option<ComponentId>, event: Event) -> bool {
        match event {
            Event::KeyPress(key) => {
                self.presses += 1;
                self.last = format!("{:?}+{:?}", key.modifiers, key.code);
                key.code == KeyCode::Char('q')
            },
            Event::ChannelReady => false,
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Created first, dropped last: the stream owns raw mode for as long as the
    // renderer's alternate screen is up.
    let events = EventStream::new();

    let terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let renderer = Renderer::new(terminal)?;

    let mut app = Demo { presses: 0, last: "none".to_string(), viewport: Rect::default() };
    RenderLoop::new(renderer, EventRouter::new(), events).run(&mut app)?;

    Ok(())
}
