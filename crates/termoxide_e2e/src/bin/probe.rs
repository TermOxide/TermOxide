//! A minimal TermOxide application for the pseudo-terminal tests in
//! `tests/pty.rs`.
//!
//! It shows a fixed title, how many keys it has counted and its viewport, so a
//! test reading the screen can tell each of them apart. `q` and Ctrl-C quit,
//! `p` panics in `handle_event`, `t` makes the next `track_view` run panic, and
//! every other key is counted.

use any_spawner::Executor;
use color_eyre::Result;
use ratatui::layout::Rect;
use termoxide::{App, run_with_app};
use termoxide_event::event::{Event, KeyCode, KeyModifiers};
use termoxide_reactive::Signal;
use termoxide_rendering::view_node::ViewNode;

/// First line of every frame.
const TITLE: &str = "termoxide e2e probe";

/// Panic from a function of its own, so a trace of the panic holds two probe
/// frames: this one, then the `App` method that called it.
#[allow(clippy::panic, reason = "the probe panics on request")]
fn explode(message: &str) -> ! { panic!("{message}") }

#[derive(Clone, Copy)]
struct Probe {
    keys: Signal<u32>,
    /// Set by `t`: the write re-runs `track_view`, which then panics.
    doomed: Signal<bool>,
}

impl App for Probe {
    fn track_view(&self) {
        let _ = self.keys.get();
        if self.doomed.get() {
            explode("probe panicked in track_view on purpose");
        }
    }

    fn on_tick(&self) {}

    fn handle_event(&self, event: Event) -> bool {
        let Event::KeyPress(key) = event else {
            return false;
        };
        match key.code {
            KeyCode::Char('q') => true,
            KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => true,
            KeyCode::Char('p') => explode("probe panicked in handle_event on purpose"),
            KeyCode::Char('t') => {
                self.doomed.set(true);
                false
            },
            _ => {
                self.keys.update(|keys| *keys += 1);
                false
            },
        }
    }

    fn build_view(&self, viewport: Rect) -> ViewNode {
        let lines = [
            TITLE.to_string(),
            format!("keys: {}", self.keys.get_untracked()),
            format!("viewport: {}x{}", viewport.width, viewport.height),
        ];
        let rows = lines.into_iter().zip(0..viewport.height).map(|(line, row)| {
            ViewNode::text(
                Rect::new(viewport.x, viewport.y + row, viewport.width, 1),
                line,
                Default::default(),
            )
        });
        ViewNode::container(viewport, rows.collect())
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    color_eyre::install()?;
    Executor::init_tokio()?;

    let local = tokio::task::LocalSet::new();
    local
        .run_until(run_with_app(Probe { keys: Signal::new(0), doomed: Signal::new(false) }))
        .await
}
