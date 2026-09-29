//! Drives the real loop on tokio's paused clock: time only moves when every
//! task waits on a timer, so the loop's cadence is exact and seconds of loop
//! time run instantly.

use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    future::Future,
    io,
    rc::Rc,
    time::Duration,
};

use color_eyre::Result;
use ratatui::{
    Terminal,
    backend::{Backend, ClearType, TestBackend, WindowSize},
    buffer::Cell as BufferCell,
    layout::Rect,
};
use termoxide::{App, EventSource};
use termoxide_event::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use termoxide_reactive::Signal;
use termoxide_rendering::{renderer::Renderer, view_node::ViewNode};
use tokio::time::Instant;

pub fn ms(millis: u64) -> Duration { Duration::from_millis(millis) }

pub fn key(c: char) -> Event { Event::KeyPress(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)) }

/// Run `body` on a `LocalSet`, where the render effect spawns its re-runs.
pub async fn local<F: Future>(body: F) -> F::Output {
    static EXECUTOR: std::sync::Once = std::sync::Once::new();
    EXECUTOR.call_once(|| any_spawner::Executor::init_tokio().expect("init tokio executor"));
    tokio::task::LocalSet::new().run_until(body).await
}

/// Drive `app` through the loop until it stops; fails, rather than hangs, when
/// the loop never stops.
pub async fn run(app: &ProbeApp, backend: &SharedBackend, events: ScriptedEvents) -> Result<()> {
    let renderer = Renderer::new_for_test(Terminal::new(backend.clone()).expect("terminal"));
    let run = termoxide::run_with_backend(app.clone(), renderer, events);
    tokio::time::timeout(Duration::from_secs(60), run)
        .await
        .expect("the loop never stopped")
}

/// A [`TestBackend`] the test can read and resize while the loop owns it.
#[derive(Clone)]
pub struct SharedBackend {
    inner: Rc<RefCell<TestBackend>>,
    fail_draw: Rc<Cell<bool>>,
}

impl SharedBackend {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            inner: Rc::new(RefCell::new(TestBackend::new(width, height))),
            fail_draw: Rc::default(),
        }
    }

    /// Make every later `draw` fail.
    pub fn fail_draws(&self) { self.fail_draw.set(true); }

    pub fn resize(&self, width: u16, height: u16) { self.inner.borrow_mut().resize(width, height); }

    pub fn assert_lines(&self, expected: &[&str]) { self.inner.borrow().assert_buffer_lines(expected.iter().copied()); }
}

impl Backend for SharedBackend {
    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a BufferCell)>,
    {
        if self.fail_draw.get() {
            return Err(io::Error::other("scripted draw failure"));
        }
        self.inner.borrow_mut().draw(content)
    }

    fn append_lines(&mut self, n: u16) -> io::Result<()> { self.inner.borrow_mut().append_lines(n) }

    fn hide_cursor(&mut self) -> io::Result<()> { self.inner.borrow_mut().hide_cursor() }

    fn show_cursor(&mut self) -> io::Result<()> { self.inner.borrow_mut().show_cursor() }

    fn get_cursor(&mut self) -> io::Result<(u16, u16)> { self.inner.borrow_mut().get_cursor() }

    fn set_cursor(&mut self, x: u16, y: u16) -> io::Result<()> { self.inner.borrow_mut().set_cursor(x, y) }

    fn clear(&mut self) -> io::Result<()> { self.inner.borrow_mut().clear() }

    fn clear_region(&mut self, clear_type: ClearType) -> io::Result<()> {
        self.inner.borrow_mut().clear_region(clear_type)
    }

    fn size(&self) -> io::Result<Rect> { self.inner.borrow().size() }

    fn window_size(&mut self) -> io::Result<WindowSize> { self.inner.borrow_mut().window_size() }

    fn flush(&mut self) -> io::Result<()> { self.inner.borrow_mut().flush() }
}

/// Hands out each event at the first poll at or after its offset from creation.
pub struct ScriptedEvents {
    start: Instant,
    timeline: RefCell<VecDeque<(Duration, Event)>>,
    teardown_error: Option<termoxide_event::Error>,
    /// How many times `teardown` ran.
    pub teardowns: Rc<Cell<u32>>,
}

impl ScriptedEvents {
    pub fn new(timeline: impl IntoIterator<Item = (Duration, Event)>) -> Self {
        Self {
            start: Instant::now(),
            timeline: RefCell::new(timeline.into_iter().collect()),
            teardown_error: None,
            teardowns: Rc::default(),
        }
    }

    pub fn failing_teardown(mut self) -> Self {
        self.teardown_error = Some(termoxide_event::Error::Terminal(io::Error::other("scripted reader failure")));
        self
    }
}

impl EventSource for ScriptedEvents {
    fn poll_events(&self) -> Vec<Event> {
        let elapsed = Instant::now() - self.start;
        let mut timeline = self.timeline.borrow_mut();
        let mut due = Vec::new();
        while let Some(&(offset, event)) = timeline.front()
            && offset <= elapsed
        {
            due.push(event);
            timeline.pop_front();
        }
        due
    }

    fn teardown(self) -> termoxide_event::Result<()> {
        self.teardowns.set(self.teardowns.get() + 1);
        self.teardown_error.map_or(Ok(()), Err)
    }
}

/// Every call the loop made to a [`ProbeApp`], with its virtual time.
#[derive(Default)]
pub struct Probe {
    pub ticks: RefCell<Vec<Instant>>,
    pub frames: RefCell<Vec<(Instant, Rect)>>,
    pub events: RefCell<Vec<(Instant, Event)>>,
}

impl Probe {
    pub fn tick_count(&self) -> usize { self.ticks.borrow().len() }

    pub fn frame_count(&self) -> usize { self.frames.borrow().len() }

    pub fn delivered(&self) -> Vec<Event> { self.events.borrow().iter().map(|&(_, event)| event).collect() }
}

/// Shows a key counter and its viewport, records every call, quits on `q`, and
/// counts `a`.
#[derive(Clone)]
pub struct ProbeApp {
    pub count: Signal<u32>,
    pub probe: Rc<Probe>,
}

impl ProbeApp {
    pub fn new() -> Self { Self { count: Signal::new(0), probe: Rc::default() } }
}

impl App for ProbeApp {
    fn track_view(&self) { let _ = self.count.get(); }

    fn on_tick(&self) { self.probe.ticks.borrow_mut().push(Instant::now()); }

    fn handle_event(&self, event: Event) -> bool {
        self.probe.events.borrow_mut().push((Instant::now(), event));
        let Event::KeyPress(pressed) = event else {
            return false;
        };
        match pressed.code {
            KeyCode::Char('q') => true,
            KeyCode::Char('a') => {
                self.count.update(|count| *count += 1);
                false
            },
            _ => false,
        }
    }

    fn build_view(&self, viewport: Rect) -> ViewNode {
        self.probe.frames.borrow_mut().push((Instant::now(), viewport));
        let line = |row: u16, content: String| {
            ViewNode::text(
                Rect::new(viewport.x, viewport.y + row, viewport.width, 1),
                content,
                Default::default(),
            )
        };
        ViewNode::container(viewport, vec![
            line(0, format!("count: {}", self.count.get_untracked())),
            line(1, format!("{}x{}", viewport.width, viewport.height)),
        ])
    }
}
