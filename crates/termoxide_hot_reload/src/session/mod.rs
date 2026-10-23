use std::{cell::Cell, rc::Rc};

use reactive_graph::effect::RenderEffect;
use serde::{Serialize, de::DeserializeOwned};
use termoxide::App;
use termoxide_reactive::Owner;

use crate::{
    ipc::{self, IpcError, Outcome, Request, Response},
    schema,
};

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
    /// The fingerprint of `A`, carried by the state so a generation with
    /// another state type doesn't decode it. `None` when `A` can't be traced.
    fingerprint: Option<u64>,
}

impl<A: App + Clone + Serialize + DeserializeOwned + 'static> Responder<A> {
    pub(crate) fn new(app: A) -> Self {
        let fingerprint = schema::fingerprint::<A>()
            .map_err(|e| log::warn!("a change to the app's state type will go unnoticed, it can't be traced: {e}"))
            .ok();
        Self { fresh: Some(app), session: None, fingerprint }
    }

    /// Starts working on `request`.
    pub(crate) fn step(&mut self, request: Request) -> ipc::Result<Step> {
        if let Some(session) = &mut self.session {
            return session.apply(request);
        }
        match request {
            Request::Ping => Ok(Step::Reply(Response::Pong)),
            Request::Start { state } => {
                let app = self.fresh.take().ok_or(IpcError::Unexpected)?;
                let (session, started) = Session::start(app, state.as_deref(), self.fingerprint)?;
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
    fingerprint: Option<u64>,
    _effect: RenderEffect<()>,
    _owner: Owner,
}

impl<A: App + Clone + Serialize + DeserializeOwned + 'static> Session<A> {
    /// Starts the app: `fresh` as built, or decoded from the host's `state`
    /// unless another state type saved it.
    fn start(fresh: A, state: Option<&[u8]>, fingerprint: Option<u64>) -> ipc::Result<(Self, Response)> {
        let owner = Owner::new();
        owner.set();

        let (app, reset) = match state.map(|state| decode(state, fingerprint)).transpose()? {
            Some(Some(app)) => (app, false),
            Some(None) => (fresh, true),
            None => (fresh, false),
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

        let sent = encode(&app, fingerprint)?;
        let started = Response::Started { mouse_capture: app.mouse_capture(), state: sent.clone(), reset };
        Ok((Self { app, dirty, sent, fingerprint, _effect: effect, _owner: owner }, started))
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
        let encoded = encode(&self.app, self.fingerprint)?;
        let state = (encoded != self.sent).then(|| {
            self.sent.clone_from(&encoded);
            encoded
        });
        Ok(Response::Done(Outcome { quit, dirty: self.dirty.replace(false), state }))
    }
}

/// The app's state as the host keeps it: the fingerprint of its type, then the
/// app.
fn encode<A: Serialize>(app: &A, fingerprint: Option<u64>) -> ipc::Result<Vec<u8>> {
    let mut state = fingerprint.unwrap_or_default().to_le_bytes().to_vec();
    state.extend(postcard::to_allocvec(app).map_err(IpcError::Encode)?);
    Ok(state)
}

/// The app in `state`, or `None` when another state type saved it. A state
/// that matches the type yet doesn't decode is an error, not a reset.
fn decode<A: DeserializeOwned>(state: &[u8], fingerprint: Option<u64>) -> ipc::Result<Option<A>> {
    let Some((saved, app)) = state.split_first_chunk::<8>() else {
        return Ok(None);
    };
    if fingerprint.is_some_and(|fingerprint| fingerprint != u64::from_le_bytes(*saved)) {
        return Ok(None);
    }
    postcard::from_bytes(app).map(Some).map_err(IpcError::Decode)
}

#[cfg(test)]
mod tests;
