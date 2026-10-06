//! Drives the real loop on tokio's paused clock: time only moves when every
//! task waits on a timer, so the loop's cadence is exact and seconds of loop
//! time run instantly.

use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    future::Future,
    io,
    panic::Location,
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

/// Drive `app` through the loop until it stops, without a stack trace.
pub async fn run(app: &ProbeApp, backend: &SharedBackend, events: ScriptedEvents) -> Result<()> {
    run_traced(app, backend, events, false).await
}

/// Fails, rather than hangs, when the loop never stops.
pub async fn run_traced(app: &ProbeApp, backend: &SharedBackend, events: ScriptedEvents, trace: bool) -> Result<()> {
    let renderer = Renderer::new_for_test(Terminal::new(backend.clone()).expect("terminal"));
    let run = termoxide::run_with_backend(app.clone(), renderer, events, trace);
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

#[derive(Clone, Copy, Debug)]
pub struct PanicSite {
    pub at: Instant,
    pub file: &'static str,
    pub line: u32,
    pub column: u32,
}

/// Every call the loop made to a [`ProbeApp`], with its virtual time.
#[derive(Default)]
pub struct Probe {
    pub ticks: RefCell<Vec<Instant>>,
    pub frames: RefCell<Vec<(Instant, Rect)>>,
    pub events: RefCell<Vec<(Instant, Event)>>,
    pub panicked: Cell<Option<PanicSite>>,
}

impl Probe {
    pub fn tick_count(&self) -> usize { self.ticks.borrow().len() }

    pub fn frame_count(&self) -> usize { self.frames.borrow().len() }

    pub fn delivered(&self) -> Vec<Event> { self.events.borrow().iter().map(|&(_, event)| event).collect() }

    pub fn assert_nothing_after_the_panic(&self) {
        let at = self.panicked.get().expect("the probe panicked").at;
        assert!(self.ticks.borrow().iter().all(|&time| time <= at), "a tick after the panic");
        assert!(
            self.frames.borrow().iter().all(|&(time, _)| time <= at),
            "a frame after the panic"
        );
        assert!(
            self.events.borrow().iter().all(|&(time, _)| time <= at),
            "an event after the panic"
        );
    }
}

/// Where a [`ProbeApp`] panics; nowhere by default. Counts start at 1.
#[derive(Clone, Copy, Default)]
pub struct Panics {
    pub on_key: Option<char>,
    pub on_tick: Option<usize>,
    pub on_frame: Option<usize>,
    pub in_first_track: bool,
    pub once_count_reaches: Option<u32>,
}

/// Panics with `message`; `#[track_caller]` makes the caller the panic's
/// location, which is recorded in `probe`.
#[track_caller]
fn probe_panic(probe: &Probe, message: &str) -> ! {
    let caller = Location::caller();
    probe.panicked.set(Some(PanicSite {
        at: Instant::now(),
        file: caller.file(),
        line: caller.line(),
        column: caller.column(),
    }));
    panic!("{message}")
}

/// Shows a key counter and its viewport, records every call, quits on `q`,
/// counts `a`, and panics where [`Panics`] says.
#[derive(Clone)]
pub struct ProbeApp {
    pub count: Signal<u32>,
    panics: Panics,
    pub probe: Rc<Probe>,
}

impl ProbeApp {
    pub fn new() -> Self { Self::panicking(Panics::default()) }

    pub fn panicking(panics: Panics) -> Self { Self { count: Signal::new(0), panics, probe: Rc::default() } }
}

impl App for ProbeApp {
    fn track_view(&self) {
        if self.panics.in_first_track {
            probe_panic(&self.probe, "track_view blew up");
        }
        let count = self.count.get();
        if self.panics.once_count_reaches.is_some_and(|limit| count >= limit) {
            probe_panic(&self.probe, "track_view blew up");
        }
    }

    fn on_tick(&self) {
        if self.panics.on_tick == Some(self.probe.tick_count() + 1) {
            probe_panic(&self.probe, "on_tick blew up");
        }
        self.probe.ticks.borrow_mut().push(Instant::now());
    }

    fn handle_event(&self, event: Event) -> bool {
        self.probe.events.borrow_mut().push((Instant::now(), event));
        let Event::KeyPress(pressed) = event else {
            return false;
        };
        match pressed.code {
            KeyCode::Char(c) if self.panics.on_key == Some(c) => probe_panic(&self.probe, "handle_event blew up"),
            KeyCode::Char('q') => true,
            KeyCode::Char('a') => {
                self.count.update(|count| *count += 1);
                false
            },
            _ => false,
        }
    }

    fn build_view(&self, viewport: Rect) -> ViewNode {
        if self.panics.on_frame == Some(self.probe.frame_count() + 1) {
            probe_panic(&self.probe, "build_view blew up");
        }
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
