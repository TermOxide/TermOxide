//! A release run: the app linked into this executable and driven by
//! termoxide's own loop, with no subprocess, tracing its state when the run
//! persists.

use std::{
    cell::{Cell, RefCell},
    io,
    rc::Rc,
};

use color_eyre::Result;
use ratatui::layout::Rect;
use termoxide::App;
use termoxide_event::event::Event;
use termoxide_reactive::recorded_only;
use termoxide_rendering::view_node::ViewNode;

use crate::{
    HotApp,
    schema,
    snapshot::{Aside, Snapshot},
    state,
};

/// The app, tracing its state after every event and tick that changed one of
/// its recorded signals.
#[derive(Clone)]
pub(crate) struct Persisting<A> {
    app: A,
    recorder: Rc<Recorder>,
}

struct Recorder {
    snapshot: Snapshot,
    fingerprint: Option<u64>,
    /// The app's recorded signals as last traced, as [`recorded_only`] encodes
    /// them.
    recorded: RefCell<Vec<u8>>,
    failed: Cell<bool>,
}

impl<A: HotApp> Persisting<A> {
    /// Starts the app from the last state traced in `snapshot`, unless `fresh`
    /// or another state type saved it, else as `build` makes it, and traces
    /// the run there.
    pub(crate) fn start(build: impl FnOnce() -> A, snapshot: Snapshot, fresh: bool) -> Result<Self> {
        let fingerprint = schema::fingerprint::<A>()
            .map_err(|e| log::warn!("a change to the app's state type will go unnoticed, it can't be traced: {e}"))
            .ok();
        let app = match restore(&snapshot, fresh, fingerprint)? {
            Some(app) => app,
            None => build(),
        };
        let recorder = Recorder { snapshot, fingerprint, recorded: RefCell::default(), failed: Cell::new(false) };
        let persisting = Self { app, recorder: Rc::new(recorder) };
        // The trace starts with the state the app started from.
        persisting.record();
        Ok(persisting)
    }

    fn record(&self) {
        if let Err(e) = self.try_record()
            && !self.recorder.failed.replace(true)
        {
            log::warn!(
                "tracing the app's state to {} failed: {e}",
                self.recorder.snapshot.path().display()
            );
        }
    }

    /// Traces the app's state if a recorded signal changed since it last did.
    fn try_record(&self) -> Result<()> {
        let Recorder { snapshot, fingerprint, recorded, .. } = &*self.recorder;
        let now = recorded_only(|| state::encode(&self.app, *fingerprint))?;
        if now == *recorded.borrow() {
            return Ok(());
        }
        snapshot.record(&state::encode(&self.app, *fingerprint)?)?;
        *recorded.borrow_mut() = now;
        Ok(())
    }
}

/// The app in the last state traced in `snapshot`, or `None` when it starts
/// afresh. A trace it can't start from is set aside.
fn restore<A: HotApp>(snapshot: &Snapshot, fresh: bool, fingerprint: Option<u64>) -> Result<Option<A>> {
    let saved = match snapshot.load(fresh) {
        Err(e) if e.kind() == io::ErrorKind::InvalidData => {
            let aside = snapshot.set_aside(Aside::Unreadable)?;
            log::warn!("{e}, so it is kept as {} and the app starts afresh", aside.display());
            return Ok(None);
        },
        loaded => loaded?,
    };
    let Some(saved) = saved else {
        return Ok(None);
    };
    match state::decode(&saved, fingerprint) {
        Ok(Some(app)) => Ok(Some(app)),
        Ok(None) => {
            log::warn!("the snapshot was saved by another state type, so the app starts afresh");
            Ok(None)
        },
        Err(e) => {
            let aside = snapshot.set_aside(Aside::Unreadable)?;
            log::warn!(
                "the app couldn't start from the snapshot: {e}, so it is kept as {} and starts afresh",
                aside.display()
            );
            Ok(None)
        },
    }
}

impl<A: HotApp> App for Persisting<A> {
    fn track_view(&self) { self.app.track_view(); }

    fn on_tick(&self) {
        self.app.on_tick();
        self.record();
    }

    fn handle_event(&self, event: Event) -> bool {
        let quit = self.app.handle_event(event);
        self.record();
        quit
    }

    fn build_view(&self, viewport: Rect) -> ViewNode { self.app.build_view(viewport) }

    fn mouse_capture(&self) -> bool { self.app.mouse_capture() }
}

#[cfg(test)]
mod tests;
