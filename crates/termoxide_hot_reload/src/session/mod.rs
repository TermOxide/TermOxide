use std::{cell::Cell, rc::Rc};

use reactive_graph::effect::RenderEffect;
use serde::{Serialize, de::DeserializeOwned};
use termoxide::App;
use termoxide_reactive::Owner;

use crate::ipc::{self, IpcError, Outcome, Request, Response};

/// What answering a request takes next.
pub(crate) enum Step {
    /// The answer, ready to send.
    Reply(Response),
    /// The app ran an event or a tick: let its render effect run, then ask
    /// [`Responder::settled`] for the answer.
    Settle { quit: bool },
}

/// Answers the host's requests, before and after the app has started.
///
/// Letting the app's effects run between [`Step::Settle`] and
/// [`settled`](Self::settled) is the caller's job: how depends on the executor
/// it runs on.
pub(crate) struct Responder<A> {
    /// The app as built, until [`Request::Start`] says whether to run it.
    fresh: Option<A>,
    session: Option<Session<A>>,
}

impl<A: App + Clone + Serialize + DeserializeOwned + 'static> Responder<A> {
    pub(crate) fn new(app: A) -> Self { Self { fresh: Some(app), session: None } }

    /// Starts working on `request`.
    pub(crate) fn step(&mut self, request: Request) -> ipc::Result<Step> {
        if let Some(session) = &mut self.session {
            return session.apply(request);
        }
        match request {
            Request::Ping => Ok(Step::Reply(Response::Pong)),
            Request::Start { state } => {
                let app = self.fresh.take().ok_or(IpcError::Unexpected)?;
                let (session, started) = Session::start(app, state.as_deref())?;
                self.session = Some(session);
                Ok(Step::Reply(started))
            },
            _ => Err(IpcError::Unexpected),
        }
    }

    /// The answer to the event or tick that asked to [`Step::Settle`].
    pub(crate) fn settled(&mut self, quit: bool) -> ipc::Result<Response> {
        self.session.as_mut().ok_or(IpcError::Unexpected)?.settled(quit)
    }
}

/// The app once started, with what it takes to tell the host when to repaint
/// and when the state changed.
struct Session<A> {
    app: A,
    /// Set by `_effect` when a signal the view reads changes.
    dirty: Rc<Cell<bool>>,
    /// The state last sent to the host.
    sent: Vec<u8>,
    _effect: RenderEffect<()>,
    _owner: Owner,
}

impl<A: App + Clone + Serialize + DeserializeOwned + 'static> Session<A> {
    /// Starts the app: `fresh` as built, or decoded from the host's `state`.
    fn start(fresh: A, state: Option<&[u8]>) -> ipc::Result<(Self, Response)> {
        let owner = Owner::new();
        owner.set();

        let app = match state {
            Some(bytes) => postcard::from_bytes(bytes).map_err(IpcError::Decode)?,
            None => fresh,
        };
        let dirty = Rc::new(Cell::new(false));
        let effect = {
            let (app, dirty) = (app.clone(), Rc::clone(&dirty));
            RenderEffect::new(move |_| {
                app.track_view();
                dirty.set(true);
            })
        };
        // The effect's first run, inside `RenderEffect::new`, isn't a change:
        // the host paints its first frame anyway.
        dirty.set(false);

        let sent = encode(&app)?;
        let started = Response::Started { mouse_capture: app.mouse_capture(), state: sent.clone() };
        Ok((Self { app, dirty, sent, _effect: effect, _owner: owner }, started))
    }

    fn apply(&mut self, request: Request) -> ipc::Result<Step> {
        Ok(match request {
            Request::Ping => Step::Reply(Response::Pong),
            Request::BuildView(viewport) => Step::Reply(Response::View(self.app.build_view(viewport))),
            Request::Event(event) => Step::Settle { quit: self.app.handle_event(event) },
            Request::Tick => {
                self.app.on_tick();
                Step::Settle { quit: false }
            },
            Request::Start { .. } | Request::Load { .. } => return Err(IpcError::Unexpected),
        })
    }

    /// Reads, once the render effect has run, whether to repaint and whether
    /// the state changed.
    fn settled(&mut self, quit: bool) -> ipc::Result<Response> {
        let encoded = encode(&self.app)?;
        let state = (encoded != self.sent).then(|| {
            self.sent.clone_from(&encoded);
            encoded
        });
        Ok(Response::Done(Outcome { quit, dirty: self.dirty.replace(false), state }))
    }
}

fn encode<A: Serialize>(app: &A) -> ipc::Result<Vec<u8>> { postcard::to_allocvec(app).map_err(IpcError::Encode) }

#[cfg(test)]
mod tests;
