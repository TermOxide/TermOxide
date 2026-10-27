use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    io,
    path::{Path, PathBuf},
    rc::Rc,
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::{Duration, Instant},
};

use color_eyre::{Result, eyre::eyre};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
};
use termoxide::App;
use termoxide_event::event::{Event, KeyCode, KeyModifiers};
use termoxide_reactive::Trigger;
use termoxide_rendering::view_node::ViewNode;

use crate::{
    ipc::{IpcError, Request, Response},
    snapshot::{Aside, Snapshot},
    subprocess::Subprocess,
    watch::Rebuilds,
};

/// How many restarts are allowed within [`RESTART_WINDOW`] before the app is
/// stopped until the next rebuild: Erlang/OTP's restart intensity, looser than
/// OTP's default of 1 in 5 s so a couple of crashes while editing don't stop it.
const MAX_RESTARTS: usize = 3;
const RESTART_WINDOW: Duration = Duration::from_secs(10);

/// The host's stand-in for the app: every call is answered by the subprocess,
/// so `termoxide`'s run loop drives it like any other app.
///
/// When the subprocess is lost, a new one starts in the background from the
/// host's copy of the state; when the app crashes too often, it is stopped
/// until the next rebuild. Meanwhile the last frame stays on screen with a
/// notice, and Ctrl+C quits, since the app's own quit key can't be reached.
#[derive(Clone)]
pub(crate) struct RemoteApp(Rc<Inner>);

struct Inner {
    link: RefCell<Link>,
    /// Notified when what the screen should show changed.
    repaint: Trigger,
    mouse_capture: bool,
    /// The app's latest state: the host's copy, to restart a lost subprocess
    /// from, unrecorded changes included.
    state: RefCell<Vec<u8>>,
    rebuilds: Rebuilds,
    /// The component the subprocess runs, to start a new one with.
    guest: RefCell<PathBuf>,
    log: PathBuf,
    restarts: RefCell<Restarts>,
    /// The last view the app built, shown while it can't answer.
    view: RefCell<Option<ViewNode>>,
    /// Where `state` is traced after every recorded change, when the run
    /// persists.
    snapshot: Option<Snapshot>,
    /// Saving has failed already: said once, not on every change.
    save_failed: Cell<bool>,
}

enum Link {
    Running(Subprocess),
    /// A new subprocess is starting in the background.
    Respawning(Receiver<Result<(Subprocess, Vec<u8>)>>),
    /// The app crashed too often: waiting for the next rebuild.
    Stopped,
}

impl RemoteApp {
    /// Starts the app in a subprocess running `guest` and logging to `log`,
    /// and swaps in each generation `rebuilds` builds. With a `snapshot`, it
    /// starts from the last state the last run traced there, unless `fresh`,
    /// and traces this run there.
    pub(crate) fn start(
        rebuilds: Rebuilds,
        guest: PathBuf,
        log: PathBuf,
        snapshot: Option<Snapshot>,
        fresh: bool,
    ) -> Result<Self> {
        let saved = match &snapshot {
            Some(snapshot) => {
                match snapshot.load(fresh) {
                    Err(e) if e.kind() == io::ErrorKind::InvalidData => {
                        let aside = snapshot.set_aside(Aside::Unreadable)?;
                        log::warn!("{e}, so it is kept as {} and the app starts afresh", aside.display());
                        None
                    },
                    loaded => loaded?,
                }
            },
            None => None,
        };
        let restored = saved.is_some();
        let started = match start_subprocess(&guest, &log, saved) {
            Err(e) if restored => {
                // The snapshot matched the state type, so it ought to decode:
                // it was damaged.
                if let Some(snapshot) = &snapshot {
                    let aside = snapshot.set_aside(Aside::Unreadable)?;
                    log::warn!("the app couldn't start from the snapshot: {e:#}, the subprocess's error is above");
                    log::warn!("so the snapshot is kept as {} and the app starts afresh", aside.display());
                }
                start_subprocess(&guest, &log, None)?
            },
            started => started?,
        };
        if started.reset {
            log::warn!("the snapshot was saved by another state type, so the app starts afresh");
        }
        let Started { subprocess, mouse_capture, state, .. } = started;
        let app = Self(Rc::new(Inner {
            link: RefCell::new(Link::Running(subprocess)),
            repaint: Trigger::new(),
            mouse_capture,
            state: RefCell::new(Vec::new()),
            rebuilds,
            guest: RefCell::new(guest),
            log,
            restarts: RefCell::new(Restarts::default()),
            view: RefCell::new(None),
            snapshot,
            save_failed: Cell::new(false),
        }));
        // The trace starts with the state the app started from.
        app.set_state(state, true);
        Ok(app)
    }

    /// Keeps `state` as the host's copy, and traces it when the run persists
    /// and the change is `recorded`.
    fn set_state(&self, state: Vec<u8>, recorded: bool) {
        if recorded
            && let Some(snapshot) = &self.0.snapshot
            && let Err(e) = snapshot.record(&state)
            && !self.0.save_failed.replace(true)
        {
            log::warn!("tracing the app's state to {} failed: {e}", snapshot.path().display());
        }
        *self.0.state.borrow_mut() = state;
    }

    /// Sends `request` to a running subprocess. A subprocess that can't
    /// answer is lost: this replaces it and returns `None`, the request
    /// dropped.
    fn request(&self, request: &Request) -> Option<Response> {
        let answer = match &mut *self.0.link.borrow_mut() {
            Link::Running(subprocess) => subprocess.request(request),
            Link::Respawning(_) | Link::Stopped => return None,
        };
        match answer {
            Ok(response) => Some(response),
            Err(e) => {
                self.lose(&e.into());
                None
            },
        }
    }

    /// Sends an event or a tick, keeping whatever it changed. Returns whether
    /// the app asked to quit.
    fn apply(&self, request: &Request) -> bool {
        match self.request(request) {
            Some(Response::Done(outcome)) => {
                if let Some(state) = outcome.state {
                    self.set_state(state, outcome.recorded);
                }
                if outcome.dirty {
                    self.0.repaint.notify();
                }
                outcome.quit
            },
            // A new generation took this tick's place: its view may differ even
            // if no signal changed.
            Some(Response::Reloaded { state }) => {
                log::info!("reloaded the app");
                self.0.restarts.borrow_mut().reset();
                self.set_state(state, true);
                self.0.repaint.notify();
                false
            },
            Some(Response::Recovered { state }) => {
                self.recovered(state);
                false
            },
            Some(_) => {
                self.lose(&IpcError::Unexpected.into());
                false
            },
            None => false,
        }
    }

    /// The subprocess restarted the app after a crash: counts it.
    fn recovered(&self, state: Vec<u8>) {
        self.set_state(state, true);
        self.0.repaint.notify();
        if !self.0.restarts.borrow_mut().allow(Instant::now()) {
            self.stop();
        }
    }

    /// The subprocess can't answer: drops it, killing it, and starts another
    /// unless that would be one restart too many.
    fn lose(&self, error: &color_eyre::Report) {
        log::error!("lost the subprocess: {error:#}");
        if self.0.restarts.borrow_mut().allow(Instant::now()) {
            self.respawn();
        } else {
            self.stop();
        }
    }

    /// Starts a new subprocess in the background, from the host's state.
    fn respawn(&self) {
        log::info!("restarting the subprocess");
        let (guest, log, state) = (self.0.guest.borrow().clone(), self.0.log.clone(), self.0.state.borrow().clone());
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let _ =
                tx.send(start_subprocess(&guest, &log, Some(state)).map(|started| (started.subprocess, started.state)));
        });
        // Dropping the running subprocess, if any, kills it.
        *self.0.link.borrow_mut() = Link::Respawning(rx);
        self.0.repaint.notify();
    }

    fn stop(&self) {
        log::error!(
            "the app crashed {MAX_RESTARTS} times within {RESTART_WINDOW:?}, so it is stopped until the next rebuild"
        );
        *self.0.link.borrow_mut() = Link::Stopped;
        self.0.repaint.notify();
        // The latest state may be what crashes it: the next run mustn't start
        // from it.
        if let Some(snapshot) = &self.0.snapshot {
            match snapshot.set_aside(Aside::Crashed) {
                Ok(aside) => {
                    log::warn!(
                        "the state trace is kept as {}, readable with termoxide_dump, and the next run starts afresh",
                        aside.display()
                    );
                },
                Err(e) => log::warn!("setting the state trace aside failed: {e}"),
            }
        }
    }

    /// Takes in the subprocess started in the background, once it is.
    fn poll_respawn(&self) {
        let started = match &*self.0.link.borrow() {
            Link::Respawning(started) => {
                match started.try_recv() {
                    Ok(started) => started,
                    Err(TryRecvError::Empty) => return,
                    Err(TryRecvError::Disconnected) => Err(eyre!("starting the subprocess stopped")),
                }
            },
            Link::Running(_) | Link::Stopped => return,
        };
        match started {
            Ok((subprocess, state)) => {
                log::info!("restarted the subprocess");
                *self.0.link.borrow_mut() = Link::Running(subprocess);
                self.set_state(state, true);
                self.0.repaint.notify();
            },
            Err(e) => self.lose(&e),
        }
    }

    /// Hands the newest component built, if any, to a running subprocess,
    /// which compiles it alongside the running generation and swaps at a later
    /// tick. A stopped app starts again with it, the restarts counted afresh.
    fn load_rebuilt(&self) {
        let Some(guest) = self.0.rebuilds.latest() else { return };
        *self.0.guest.borrow_mut() = guest.clone();
        let stopped = matches!(*self.0.link.borrow(), Link::Stopped);
        if stopped {
            self.0.restarts.borrow_mut().reset();
            self.respawn();
        } else if !matches!(self.request(&Request::Load { guest }), Some(Response::Loading) | None) {
            self.lose(&IpcError::Unexpected.into());
        }
    }

    /// The last view, with a line at the bottom saying why it isn't live.
    fn last_view(&self, viewport: Rect, notice: &str) -> ViewNode {
        let line = Rect::new(viewport.x, viewport.bottom().saturating_sub(1), viewport.width, 1);
        let notice = ViewNode::text(line, notice, Style::default().add_modifier(Modifier::REVERSED));
        ViewNode::container(viewport, self.0.view.borrow().clone().into_iter().chain([notice]).collect())
    }
}

/// A subprocess that has started the app.
struct Started {
    subprocess: Subprocess,
    mouse_capture: bool,
    state: Vec<u8>,
    /// The app started afresh: the state it was given was saved by another
    /// state type.
    reset: bool,
}

/// Starts a subprocess running `guest`, logging to `log`, from `state` or from
/// scratch.
fn start_subprocess(guest: &Path, log: &Path, state: Option<Vec<u8>>) -> Result<Started> {
    let mut subprocess = Subprocess::spawn(log, guest)?;
    subprocess.ping()?;
    match subprocess.request(&Request::Start { state })? {
        Response::Started { mouse_capture, state, reset } => Ok(Started { subprocess, mouse_capture, state, reset }),
        _ => Err(IpcError::Unexpected.into()),
    }
}

fn is_ctrl_c(event: &Event) -> bool {
    matches!(event, Event::KeyPress(key) if key.code == KeyCode::Char('c') && key.modifiers == KeyModifiers::CONTROL)
}

impl App for RemoteApp {
    fn track_view(&self) { self.0.repaint.track(); }

    fn on_tick(&self) {
        self.poll_respawn();
        self.load_rebuilt();
        self.apply(&Request::Tick);
    }

    fn handle_event(&self, event: Event) -> bool {
        match *self.0.link.borrow() {
            Link::Running(_) => {},
            Link::Respawning(_) | Link::Stopped => return is_ctrl_c(&event),
        }
        self.apply(&Request::Event(event))
    }

    fn build_view(&self, viewport: Rect) -> ViewNode {
        match self.request(&Request::BuildView(viewport)) {
            Some(Response::View(view)) => {
                *self.0.view.borrow_mut() = Some(view.clone());
                return view;
            },
            Some(Response::Recovered { state }) => self.recovered(state),
            Some(_) => self.lose(&IpcError::Unexpected.into()),
            None => {},
        }
        let notice = match *self.0.link.borrow() {
            Link::Running(_) | Link::Respawning(_) => "the app crashed and is restarting (Ctrl+C quits)",
            Link::Stopped => "the app keeps crashing: stopped until the next rebuild (Ctrl+C quits)",
        };
        self.last_view(viewport, notice)
    }

    fn mouse_capture(&self) -> bool { self.0.mouse_capture }
}

/// Erlang/OTP-style restart intensity: at most [`MAX_RESTARTS`] within any
/// [`RESTART_WINDOW`].
#[derive(Default)]
struct Restarts {
    recent: VecDeque<Instant>,
}

impl Restarts {
    /// Counts a restart at `now`. Returns whether it stays within the limit;
    /// a restart beyond it isn't counted.
    fn allow(&mut self, now: Instant) -> bool {
        while self.recent.front().is_some_and(|&at| now.duration_since(at) >= RESTART_WINDOW) {
            self.recent.pop_front();
        }
        if self.recent.len() >= MAX_RESTARTS {
            return false;
        }
        self.recent.push_back(now);
        true
    }

    fn reset(&mut self) { self.recent.clear(); }
}

#[cfg(test)]
mod tests;
