use std::{io::stdout, time::Duration};

use color_eyre::Result;
use ratatui::{Terminal, backend::CrosstermBackend, layout::Rect};
use reactive_graph::effect::RenderEffect;
use termoxide_event::{EventStream, event::Event};
use termoxide_rendering::{renderer::Renderer, view_node::ViewNode};

pub trait App {
    fn track_view(&self);
    fn on_tick(&self);
    fn handle_event(&self, event: Event) -> bool;
    fn build_view(&self, viewport: Rect) -> ViewNode;
}

pub async fn run_with_app<A: App + Clone + 'static>(app: A) -> Result<()> {
    let owner = termoxide_reactive::Owner::new();
    owner.set();

    let redraw = std::sync::Arc::new(tokio::sync::Notify::new());
    let _redraw_effect = {
        let app_for_effect = app.clone();
        let redraw = redraw.clone();
        RenderEffect::new(move |_| {
            app_for_effect.track_view();
            redraw.notify_one();
        })
    };

    let events = EventStream::new();

    let terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let mut renderer = Renderer::new(terminal)?;

    let min_frame = Duration::from_millis(16);
    let mut last_draw = std::time::Instant::now() - min_frame;
    let mut dirty = true;
    let mut ticker = tokio::time::interval(Duration::from_millis(100));

    let result = async {
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    app.on_tick();

                    for event in events.poll_events() {
                        if app.handle_event(event) {
                            return Ok(());
                        }
                        dirty = true;
                    }

                    dirty = true;
                }
                _ = redraw.notified() => {
                    dirty = true;
                }
            }

            tokio::task::yield_now().await;

            if dirty && last_draw.elapsed() >= min_frame {
                let viewport = renderer.viewport();
                let mut root = app.build_view(viewport);
                renderer.render_frame(&mut root)?;
                dirty = false;
                last_draw = std::time::Instant::now();
            }
        }
    }
    .await;

    let _ = events.teardown();

    result
}
