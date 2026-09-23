use any_spawner::Executor;
use color_eyre::Result;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
};
use termoxide::{App, run_with_app};
use termoxide_event::event::{Event, KeyCode, KeyModifiers};
use termoxide_reactive::Signal;
use termoxide_rendering::{
    builder::{Container, NodeBuilder, el, text},
    view_node::ViewNode,
};

#[derive(Clone, Copy)]
struct AppState {
    count: Signal<u32>,
    ticks: Signal<u64>,
    last_key: Signal<String>,
    last_mouse: Signal<String>,
    mouse_capture: bool,
}

impl AppState {
    fn new(mouse_capture: bool) -> Self {
        Self {
            count: Signal::new(0),
            ticks: Signal::new(0),
            last_key: Signal::new(String::from("waiting for input")),
            last_mouse: Signal::new(Self::mouse_placeholder(mouse_capture).to_string()),
            mouse_capture,
        }
    }

    /// What the mouse line shows before any mouse event arrives.
    fn mouse_placeholder(mouse_capture: bool) -> &'static str {
        if mouse_capture { "waiting for input" } else { "not reported (--no-mouse)" }
    }

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
            Event::ChannelReady => {
                self.last_key.set(String::from("waiting for input"));
                self.last_mouse.set(Self::mouse_placeholder(self.mouse_capture).to_string());
                false
            },
            Event::KeyPress(key) => {
                self.count.update(|count| *count += 1);
                self.last_key.set(format!("key {}+{:?}", key.modifiers, key.code));
                key.code == KeyCode::Char('q')
                    || (key.code == KeyCode::Char('c') && key.modifiers == KeyModifiers::CONTROL)
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

    fn mouse_capture(&self) -> bool { self.mouse_capture }

    fn build_view(&self, viewport: Rect) -> ViewNode {
        let children: Vec<ViewNode> = [
            Self::line(
                viewport,
                0,
                "external termoxide app".to_string(),
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
                if self.mouse_capture {
                    "Controls: any key counts, the mouse is reported, q or Ctrl-C quits".to_string()
                } else {
                    "Controls: any key counts, the mouse selects text, q or Ctrl-C quits".to_string()
                },
                Style::default().fg(Color::Green),
            ),
        ]
        .into_iter()
        .flatten()
        .collect();

        el(Container).area(viewport).children(children).build()
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    color_eyre::install()?;
    Executor::init_tokio()?;

    let local = tokio::task::LocalSet::new();
    // `--no-mouse` opts out of mouse reporting, to check by hand that the
    // terminal handles text selection again.
    let mouse_capture = !std::env::args().skip(1).any(|arg| arg == "--no-mouse");

    local.run_until(run_with_app(AppState::new(mouse_capture))).await
}
