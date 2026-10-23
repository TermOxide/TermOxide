use std::path::PathBuf;

use ratatui::layout::Rect;
use serde::{Deserialize, Serialize};
use termoxide_event::event::Event;
use termoxide_rendering::view_node::ViewNode;

/// A message the host sends to the subprocess.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Request {
    /// Asks the subprocess to prove the channel works end to end.
    Ping,
    /// Starts the app, from the host's copy of its state if it has one.
    Start { state: Option<Vec<u8>> },
    /// Hands the app one input event.
    Event(Event),
    /// Advances the app's time-based state.
    Tick,
    /// Asks for the app's view of the viewport.
    BuildView(Rect),
    /// Hands the subprocess a freshly built component, to replace the running
    /// one at the next tick once it is compiled. Never reaches the component.
    Load { guest: PathBuf },
    /// Asks a component that hasn't started the app for the app in `state`,
    /// as text. What reading a state trace takes, since only the guest can
    /// decode the app.
    Describe { state: Vec<u8> },
}

/// A message the subprocess sends back to the host.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) enum Response {
    /// The answer to [`Request::Ping`].
    Pong,
    /// The answer to [`Request::Start`]: whether the app wants the mouse, its
    /// state, and whether it started afresh because the state it was given was
    /// saved by another state type.
    Started { mouse_capture: bool, state: Vec<u8>, reset: bool },
    /// The answer to [`Request::Event`] and [`Request::Tick`].
    Done(Outcome),
    /// The answer to [`Request::BuildView`].
    View(ViewNode),
    /// The answer to [`Request::Load`]: the component is compiling.
    Loading,
    /// The answer to the [`Request::Tick`] at which a new component replaced
    /// the running one, instead of that tick's [`Outcome`]: its state.
    Reloaded { state: Vec<u8> },
    /// The answer to a request the app crashed on: the request was dropped and
    /// the app restarted from its latest state, which this is.
    Recovered { state: Vec<u8> },
    /// The answer to [`Request::Describe`]: the app as pretty-printed JSON, or
    /// `None` when another state type saved it.
    Described(Option<String>),
}

/// What an event or a tick did to the app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Outcome {
    /// The app asked to stop. Always `false` after a tick.
    pub(crate) quit: bool,
    /// A signal the view reads changed, so the host must repaint.
    pub(crate) dirty: bool,
    /// The app's state, if it changed since the last one sent: a change to
    /// any of its signals counts, not only to those the view reads.
    pub(crate) state: Option<Vec<u8>>,
    /// A recorded signal changed, so `state` belongs in the state trace; an
    /// unrecorded one alone doesn't.
    pub(crate) recorded: bool,
}
