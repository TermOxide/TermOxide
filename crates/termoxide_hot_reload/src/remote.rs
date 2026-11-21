use std::{cell::RefCell, rc::Rc};

use ratatui::layout::Rect;
use termoxide::App;
use termoxide_event::event::Event;
use termoxide_reactive::Trigger;
use termoxide_rendering::view_node::ViewNode;

use crate::{
    ipc::{self, IpcError, Request, Response},
    subprocess::Subprocess,
};

/// The host's stand-in for the app: every call is answered by the subprocess,
/// so `termoxide`'s run loop drives it like any other app.
#[derive(Clone)]
pub(crate) struct RemoteApp(Rc<Inner>);

struct Inner {
    subprocess: RefCell<Subprocess>,
    /// Notified when the subprocess reports a change the view reads.
    repaint: Trigger,
    mouse_capture: bool,
    /// The app's latest state: the host's copy, to restart a lost subprocess from.
    state: RefCell<Vec<u8>>,
}

impl RemoteApp {
    /// Starts the app in `subprocess`, from scratch.
    pub(crate) fn start(mut subprocess: Subprocess) -> ipc::Result<Self> {
        let Response::Started { mouse_capture, state } = subprocess.request(&Request::Start { state: None })? else {
            return Err(IpcError::Unexpected);
        };
        Ok(Self(Rc::new(Inner {
            subprocess: RefCell::new(subprocess),
            repaint: Trigger::new(),
            mouse_capture,
            state: RefCell::new(state),
        })))
    }

    fn request(&self, request: &Request) -> Response {
        self.0.subprocess.borrow_mut().request(request).unwrap_or_else(|e| lost(&e))
    }

    /// Sends an event or a tick, keeping whatever it changed. Returns whether
    /// the app asked to quit.
    fn apply(&self, request: &Request) -> bool {
        let Response::Done(outcome) = self.request(request) else {
            lost(&IpcError::Unexpected)
        };
        if let Some(state) = outcome.state {
            *self.0.state.borrow_mut() = state;
        }
        if outcome.dirty {
            self.0.repaint.notify();
        }
        outcome.quit
    }
}

/// Ends the run: the subprocess can't answer.
#[expect(
    clippy::panic,
    reason = "until a lost subprocess is respawned; the run loop shuts down cleanly on an app panic"
)]
fn lost(error: &IpcError) -> ! { panic!("hot-reload subprocess lost: {error}") }

impl App for RemoteApp {
    fn track_view(&self) { self.0.repaint.track(); }

    fn on_tick(&self) { self.apply(&Request::Tick); }

    fn handle_event(&self, event: Event) -> bool { self.apply(&Request::Event(event)) }

    fn build_view(&self, viewport: Rect) -> ViewNode {
        let Response::View(view) = self.request(&Request::BuildView(viewport)) else {
            lost(&IpcError::Unexpected)
        };
        view
    }

    fn mouse_capture(&self) -> bool { self.0.mouse_capture }
}
