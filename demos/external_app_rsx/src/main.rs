use any_spawner::Executor;
use color_eyre::Result;
use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Widget},
};
use termoxide::{App, run_with_app};
use termoxide_components::{Button, ButtonProps, Msg, ProgressBar, ProgressBarProps};
use termoxide_event::event::{Event, KeyCode, KeyModifiers, MouseButton, MouseEventKind};
use termoxide_macros::rsx;
use termoxide_reactive::Signal;
use termoxide_rendering::{builder::NodeBuilder, view_node::ViewNode};

#[derive(Clone, Copy)]
struct AppState {
    count: Signal<u32>,
    progress: Signal<u16>,
    ticks: Signal<u64>,
    last_key: Signal<String>,
    last_mouse: Signal<String>,
    button_area: Signal<Rect>,
    mouse_capture: bool,
}

impl AppState {
    fn new(mouse_capture: bool) -> Self {
        Self {
            count: Signal::new(0),
            progress: Signal::new(0),
            ticks: Signal::new(0),
            last_key: Signal::new(String::from("waiting for input")),
            last_mouse: Signal::new(Self::mouse_placeholder(mouse_capture).to_string()),
            button_area: Signal::new(Rect::default()),
            mouse_capture,
        }
    }

    fn mouse_placeholder(mouse_capture: bool) -> &'static str {
        if mouse_capture { "waiting for input" } else { "not reported (--no-mouse)" }
    }

    fn advance_progress(&self, amount: u16) {
        self.progress.update(|progress| {
            *progress = if progress.saturating_add(amount) > 100 {
                0
            } else {
                progress.saturating_add(amount)
            };
        });
    }
}

impl App for AppState {
    fn track_view(&self) {
        let _ = self.count.get();
        let _ = self.progress.get();
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
                self.advance_progress(1);
                self.last_key.set(format!("key {}+{:?}", key.modifiers, key.code));
                key.code == KeyCode::Char('q')
                    || (key.code == KeyCode::Char('c') && key.modifiers == KeyModifiers::CONTROL)
            },
            Event::Mouse(mouse) => {
                if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
                    && self
                        .button_area
                        .get_untracked()
                        .contains(Position::new(mouse.column, mouse.row))
                {
                    self.advance_progress(2);
                }
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
        let title_area = Rect::new(viewport.x, viewport.y, viewport.width, 1);
        let status_area = Rect::new(viewport.x, viewport.y.saturating_add(1), viewport.width, 1);
        let mouse_area = Rect::new(viewport.x, viewport.y.saturating_add(2), viewport.width, 1);
        let controls_area = Rect::new(viewport.x, viewport.y.saturating_add(3), viewport.width, 1);
        let button_area = Rect::new(viewport.x, viewport.y.saturating_add(4), viewport.width, 1);
        self.button_area.set(button_area);
        let progress_area = Rect::new(viewport.x, viewport.y.saturating_add(5), viewport.width, 1);
        let panel_style = Style::default().bg(Color::Rgb(18, 24, 38));
        let title_style = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
        let status_style = Style::default().fg(Color::Yellow).bg(Color::Rgb(30, 35, 52));
        let controls_style = Style::default().fg(Color::Green).add_modifier(Modifier::DIM);
        let raw_area = Rect::new(viewport.x, viewport.y.saturating_add(6), viewport.width, 3);

        rsx! {
            <Container area={viewport} style={panel_style}>
                <text area={title_area} style={title_style}>
                    {"external termoxide app"}
                </text>
                <text area={status_area} style={status_style}>
                    {format!(
                        "ticks: {} | key presses: {} | last key: {}",
                        self.ticks.get_untracked(),
                        self.count.get_untracked(),
                        self.last_key.get_untracked(),
                    )}
                </text>
                <text area={mouse_area} style={Style::default().fg(Color::Magenta)}>
                    {format!("last mouse event: {}", self.last_mouse.get_untracked())}
                </text>
                <text area={controls_area} style={controls_style}>
                    {if self.mouse_capture {
                        "Controls: any key counts, the mouse is reported, q or Ctrl-C quits"
                    } else {
                        "Controls: any key counts, the mouse selects text, q or Ctrl-C quits"
                    }}
                </text>
                <Button
                    label={"  RUN  "}
                    msg={Msg::Run}
                    area={button_area}
                    style={Style::default().fg(Color::Black).bg(Color::Magenta).add_modifier(Modifier::BOLD)}
                />
                <ProgressBar
                    value={self.progress.get_untracked()}
                    max={100u16}
                    label={format!("progress: {}%", self.progress.get_untracked())}
                    area={progress_area}
                    style={Style::default().fg(Color::Black).bg(Color::Green)}
                />
                <Raw
                    area={raw_area}
                    draw={|buffer: &mut Buffer, area: Rect| {
                        Block::default()
                            .title(" Raw runtime primitive ")
                            .borders(Borders::ALL)
                            .border_style(Style::default().fg(Color::Blue))
                            .render(area, buffer);
                    }}
                />
            </Container>
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    color_eyre::install()?;
    Executor::init_tokio()?;

    let mouse_capture = !std::env::args().skip(1).any(|arg| arg == "--no-mouse");
    let local = tokio::task::LocalSet::new();
    local.run_until(run_with_app(AppState::new(mouse_capture))).await
}
