use any_spawner::Executor;
use color_eyre::Result;
use ratatui::{layout::Rect, style::Style};
use termoxide::App;
use termoxide_event::event::{Event, KeyCode};
use termoxide_hot_reload::run_app;
use termoxide_reactive::Signal;
use termoxide_rendering::{
    builder::{Container, NodeBuilder, el, text},
    view_node::ViewNode,
};

#[derive(Clone, Copy)]
struct Counter {
    ticks: Signal<u64>,
}

impl App for Counter {
    fn track_view(&self) { let _ = self.ticks.get(); }

    fn on_tick(&self) { self.ticks.update(|ticks| *ticks += 1); }

    fn handle_event(&self, event: Event) -> bool {
        matches!(event, Event::KeyPress(key) if key.code == KeyCode::Char('q'))
    }

    fn build_view(&self, viewport: Rect) -> ViewNode {
        let line = text(format!("ticks: {} (q quits)", self.ticks.get_untracked()))
            .area(Rect::new(viewport.x, viewport.y, viewport.width, 1))
            .style(Style::default())
            .build();

        el(Container).area(viewport).children(vec![line]).build()
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    color_eyre::install()?;
    Executor::init_tokio()?;

    let local = tokio::task::LocalSet::new();
    local.run_until(run_app(Counter { ticks: Signal::new(0) })).await
}
