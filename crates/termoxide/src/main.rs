mod app;

use any_spawner::Executor;
use color_eyre::Result;
use std::{io::stdout, time::Duration};

use ratatui::{Terminal, backend::CrosstermBackend};
use reactive_graph::effect::RenderEffect;
use termoxide_event::EventStream;
use termoxide_rendering::renderer::Renderer;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    color_eyre::install()?;
    Executor::init_tokio()?;

    let local = tokio::task::LocalSet::new();
    local.run_until(run()).await
}

async fn run() -> Result<()> {
    let owner = termoxide_reactive::Owner::new();
    owner.set();

    let app = app::AppState::new();

    let redraw = std::sync::Arc::new(tokio::sync::Notify::new());
    let _redraw_effect = {
        let redraw = redraw.clone();
        RenderEffect::new(move |_| {
            app.track_view();
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
