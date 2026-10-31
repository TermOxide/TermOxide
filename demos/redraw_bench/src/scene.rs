//! The situations being measured, and the application that plays them.

use std::{cell::RefCell, rc::Rc, str::FromStr, time::Instant};

use color_eyre::eyre::{Report, bail};
use ratatui::{
    layout::Rect,
    style::{Color, Style},
};
use termoxide::App;
use termoxide_event::event::{Event, KeyCode, KeyModifiers};
use termoxide_reactive::Signal;
use termoxide_rendering::{
    builder::{Container, NodeBuilder, el, text},
    view_node::ViewNode,
};

/// Node count of the [`Scenario::Many`] screen.
const MANY_NODES: usize = 1000;

/// A situation to measure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scenario {
    /// Nothing ever changes.
    Idle,
    /// One line changes on every tick, the rhythm of the demo apps.
    Typical,
    /// One line changes as fast as the framework redraws.
    Small,
    /// Every cell changes as fast as the framework redraws.
    Full,
    /// A large tree redraws as fast as the framework allows, with two cells changing.
    Many,
}

impl Scenario {
    /// Whether the scenario needs a redraw as often as the framework can do one.
    pub const fn is_saturated(self) -> bool { matches!(self, Self::Small | Self::Full | Self::Many) }

    /// Name used on the command line and in the results.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Typical => "typical",
            Self::Small => "small",
            Self::Full => "full",
            Self::Many => "many",
        }
    }
}

impl FromStr for Scenario {
    type Err = Report;

    fn from_str(name: &str) -> Result<Self, Report> {
        match name {
            "idle" => Ok(Self::Idle),
            "typical" => Ok(Self::Typical),
            "small" => Ok(Self::Small),
            "full" => Ok(Self::Full),
            "many" => Ok(Self::Many),
            other => bail!("unknown scenario `{other}`"),
        }
    }
}

/// Build the screen of `scenario` for frame number `frame`.
pub fn view(scenario: Scenario, frame: u64, viewport: Rect) -> ViewNode {
    let children = match scenario {
        Scenario::Idle | Scenario::Typical | Scenario::Small => small_lines(frame, viewport),
        Scenario::Full => full_lines(frame, viewport),
        Scenario::Many => many_nodes(frame, viewport),
    };

    el(Container).area(viewport).children(children).build()
}

fn line(viewport: Rect, row: u16, content: String, color: Color) -> ViewNode {
    text(content)
        .area(Rect::new(viewport.x, viewport.y + row, viewport.width, 1))
        .style(Style::default().fg(color))
        .build()
}

/// Three lines like the demo apps, of which one changes.
fn small_lines(frame: u64, viewport: Rect) -> Vec<ViewNode> {
    let lines = [
        ("redraw bench".to_string(), Color::Cyan),
        (format!("frame: {frame}"), Color::Yellow),
        ("q or Ctrl-C stops the run".to_string(), Color::Green),
    ];

    (0..viewport.height)
        .zip(lines)
        .map(|(row, (content, color))| line(viewport, row, content, color))
        .collect()
}

/// One line per row, all filled with a letter that changes on every frame.
fn full_lines(frame: u64, viewport: Rect) -> Vec<ViewNode> {
    let letter = char::from(b'a' + (frame % 26) as u8);
    let content = letter.to_string().repeat(usize::from(viewport.width));

    (0..viewport.height)
        .map(|row| line(viewport, row, content.clone(), Color::White))
        .collect()
}

/// [`MANY_NODES`] one-cell nodes, of which one moves on every frame.
fn many_nodes(frame: u64, viewport: Rect) -> Vec<ViewNode> {
    let nodes = MANY_NODES.min(usize::from(viewport.width) * usize::from(viewport.height));
    let marked = usize::try_from(frame).unwrap_or(0) % nodes.max(1);

    (0..nodes)
        .map(|index| {
            let (row, column) = (index / usize::from(viewport.width), index % usize::from(viewport.width));
            let area = Rect::new(viewport.x + column as u16, viewport.y + row as u16, 1, 1);
            let symbol = if index == marked { "#" } else { "." };
            text(symbol).area(area).style(Style::default().fg(Color::White)).build()
        })
        .collect()
}

/// The moment each frame started, shared with the application.
///
/// Recording only happens between [`start`](Self::start) and [`stop`](Self::stop),
/// so that the warm-up does not count.
#[derive(Clone)]
pub struct FrameClock(Rc<RefCell<ClockState>>);

struct ClockState {
    recording: bool,
    starts: Vec<Instant>,
}

impl FrameClock {
    /// Reserve room for `capacity` frames.
    ///
    /// The memory is written to once here: an untouched allocation only becomes
    /// resident when first used, and the measurements would then mistake the
    /// recording itself for memory growing during the run.
    pub fn with_capacity(capacity: usize) -> Self {
        let mut starts = vec![Instant::now(); capacity];
        starts.clear();
        Self(Rc::new(RefCell::new(ClockState { recording: false, starts })))
    }

    pub fn start(&self) { self.0.borrow_mut().recording = true; }

    pub fn stop(&self) { self.0.borrow_mut().recording = false; }

    fn record(&self) {
        let mut state = self.0.borrow_mut();
        if state.recording {
            state.starts.push(Instant::now());
        }
    }

    /// The recorded start times, oldest first.
    pub fn starts(&self) -> Vec<Instant> { self.0.borrow().starts.clone() }
}

/// Plays a [`Scenario`] through the real framework loop.
#[derive(Clone)]
pub struct BenchApp {
    scenario: Scenario,
    frame: Signal<u64>,
    clock: FrameClock,
}

impl BenchApp {
    pub fn new(scenario: Scenario, clock: FrameClock) -> Self { Self { scenario, frame: Signal::new(0), clock } }

    /// Ask for a repaint, as an application changing its state would.
    pub fn advance(&self) { self.frame.update(|frame| *frame += 1); }
}

impl App for BenchApp {
    fn track_view(&self) { let _ = self.frame.get(); }

    fn on_tick(&self) {
        if self.scenario == Scenario::Typical {
            self.advance();
        }
    }

    fn handle_event(&self, event: Event) -> bool { is_quit(event) }

    fn build_view(&self, viewport: Rect) -> ViewNode {
        self.clock.record();
        view(self.scenario, self.frame.get_untracked(), viewport)
    }

    // Reporting the mouse is not being measured.
    fn mouse_capture(&self) -> bool { false }
}

/// Whether `event` asks to abandon the run.
pub fn is_quit(event: Event) -> bool {
    match event {
        Event::KeyPress(key) => {
            key.code == KeyCode::Char('q') || (key.code == KeyCode::Char('c') && key.modifiers == KeyModifiers::CONTROL)
        },
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{backend::TestBackend, buffer::Buffer};
    use termoxide_rendering::renderer::Renderer;

    use super::*;

    fn draw(scenario: Scenario, frame: u64, area: Rect) -> Buffer {
        let mut buffer = Buffer::empty(area);
        Renderer::<TestBackend>::draw_node(&view(scenario, frame, area), &mut buffer);
        buffer
    }

    fn changed_cells(scenario: Scenario, area: Rect) -> usize {
        draw(scenario, 0, area).diff(&draw(scenario, 1, area)).len()
    }

    #[test]
    fn scenario_names_round_trip() {
        for scenario in [
            Scenario::Idle,
            Scenario::Typical,
            Scenario::Small,
            Scenario::Full,
            Scenario::Many,
        ] {
            assert_eq!(scenario.name().parse::<Scenario>().expect("known"), scenario);
        }
        assert!("other".parse::<Scenario>().is_err());
    }

    #[test]
    fn full_changes_every_cell_on_every_frame() {
        let area = Rect::new(0, 0, 80, 24);

        assert_eq!(changed_cells(Scenario::Full, area), 80 * 24);
    }

    #[test]
    fn small_changes_a_single_cell_between_close_frames() {
        assert_eq!(changed_cells(Scenario::Small, Rect::new(0, 0, 80, 24)), 1);
    }

    #[test]
    fn many_builds_the_announced_tree_and_changes_two_cells() {
        let area = Rect::new(0, 0, 80, 24);

        assert_eq!(view(Scenario::Many, 0, area).children.len(), MANY_NODES);
        assert_eq!(changed_cells(Scenario::Many, area), 2);
    }

    #[test]
    fn views_stay_inside_a_tiny_terminal() {
        let area = Rect::new(0, 0, 2, 1);

        for scenario in [Scenario::Small, Scenario::Full, Scenario::Many] {
            draw(scenario, 7, area);
        }
    }

    #[test]
    fn q_and_ctrl_c_abandon_the_run() {
        use termoxide_event::event::KeyEvent;

        let press = |code, modifiers| Event::KeyPress(KeyEvent::new(code, modifiers));

        assert!(is_quit(press(KeyCode::Char('q'), KeyModifiers::NONE)));
        assert!(is_quit(press(KeyCode::Char('c'), KeyModifiers::CONTROL)));
        assert!(!is_quit(press(KeyCode::Char('c'), KeyModifiers::NONE)));
        assert!(!is_quit(Event::ChannelReady));
    }
}
