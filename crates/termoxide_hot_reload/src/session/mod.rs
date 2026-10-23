use std::{cell::Cell, rc::Rc};

use reactive_graph::effect::RenderEffect;
use serde::{Serialize, de::DeserializeOwned};
use termoxide::App;
use termoxide_reactive::Owner;

use crate::ipc::{self, IpcError, Outcome, Request, Response};

/// The app as the subprocess runs it, with what it takes to tell the host when
/// to repaint and when the state changed.
pub(crate) struct Session<A> {
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
    pub(crate) fn start(fresh: A, state: Option<&[u8]>) -> ipc::Result<(Self, Response)> {
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

    /// Runs `request` against the app.
    pub(crate) async fn handle(&mut self, request: Request) -> ipc::Result<Response> {
        let quit = match request {
            Request::Ping => return Ok(Response::Pong),
            Request::BuildView(viewport) => return Ok(Response::View(self.app.build_view(viewport))),
            Request::Event(event) => self.app.handle_event(event),
            Request::Tick => {
                self.app.on_tick();
                false
            },
            Request::Start { .. } => return Err(IpcError::Unexpected),
        };

        // A signal write schedules the render effect rather than running it,
        // so let it run before reading the flag.
        tokio::task::yield_now().await;

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
