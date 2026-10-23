//! The app as a wasm component: what [`wasm_app!`](crate::wasm_app) exports.

use std::{any::Any, cell::RefCell};

use any_spawner::{CustomExecutor, Executor, PinnedFuture, PinnedLocalFuture};
use futures::{
    executor::{LocalPool, LocalSpawner},
    task::LocalSpawnExt,
};
use log::{Level, LevelFilter, Log, Metadata, Record};
use serde::{Serialize, de::DeserializeOwned};
use termoxide::App;

use crate::{
    ipc::{IpcError, Request},
    schema,
    session::{Responder, Step},
};

#[doc(hidden)]
pub mod bindings {
    wit_bindgen::generate!({
        path: "wit",
        world: "guest",
        pub_export_macro: true,
        export_macro_name: "export_guest",
        default_bindings_module: "::termoxide_hot_reload::guest::bindings",
        runtime_path: "::termoxide_hot_reload::guest::wit_bindgen::rt",
    });
}

/// So the app's crate needs no dependency on `wit-bindgen` of its own.
#[doc(hidden)]
pub use wit_bindgen;

thread_local! {
    /// The app's [`Responder`], made on the first request. A `wasm_app!` can't
    /// name its type here, hence `Any`.
    static RESPONDER: RefCell<Option<Box<dyn Any>>> = const { RefCell::new(None) };
}

/// The fingerprint of the protocol this component speaks, or 0 if it can't be
/// traced, which matches no subprocess.
#[doc(hidden)]
pub fn wire_schema() -> u64 { schema::wire().unwrap_or_default() }

/// Answers one encoded request, building the app with `fresh` on the first one.
#[doc(hidden)]
pub fn handle<A>(request: Vec<u8>, fresh: impl FnOnce() -> A) -> Result<Vec<u8>, String>
where
    A: App + Clone + Serialize + DeserializeOwned + 'static,
{
    // Both already set from the second request on. The logger comes first, so
    // the app can log as soon as it is built.
    if log::set_logger(&GuestLogger).is_ok() {
        log::set_max_level(LevelFilter::Debug);
    }
    let _ = Executor::init_local_custom_executor(LocalExecutor::new());

    RESPONDER.with_borrow_mut(|slot| {
        let responder = slot
            .get_or_insert_with(|| Box::new(Responder::new(fresh())))
            .downcast_mut::<Responder<A>>()
            .ok_or("the guest holds another app type")?;
        let request: Request = postcard::from_bytes(&request).map_err(|e| IpcError::Decode(e).to_string())?;
        let response = match responder.step(request).map_err(|e| e.to_string())? {
            Step::Reply(response) => response,
            Step::Settle { quit } => {
                // A signal write schedules the render effect rather than
                // running it.
                Executor::poll_local();
                responder.settled(quit).map_err(|e| e.to_string())?
            },
        };
        postcard::to_allocvec(&response).map_err(|e| IpcError::Encode(e).to_string())
    })
}

/// The guest's `log` backend: hands every record to the host, which writes it to
/// the run's log file tagged as the app's.
struct GuestLogger;

impl Log for GuestLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool { metadata.level() <= LevelFilter::Debug }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let level = match record.level() {
            Level::Error => bindings::Level::Error,
            Level::Warn => bindings::Level::Warn,
            Level::Info => bindings::Level::Info,
            Level::Debug => bindings::Level::Debug,
            Level::Trace => bindings::Level::Trace,
        };
        bindings::log(level, &record.args().to_string());
    }

    fn flush(&self) {}
}

/// Runs the reactive runtime's tasks inside the guest, which has one thread and
/// no async runtime: a task runs when the guest polls, until none can progress.
struct LocalExecutor {
    pool: RefCell<LocalPool>,
    spawner: LocalSpawner,
}

impl LocalExecutor {
    fn new() -> Self {
        let pool = LocalPool::new();
        let spawner = pool.spawner();
        Self { pool: RefCell::new(pool), spawner }
    }
}

impl CustomExecutor for LocalExecutor {
    fn spawn(&self, future: PinnedFuture<()>) { self.spawn_local(future); }

    fn spawn_local(&self, future: PinnedLocalFuture<()>) {
        // Fails only once the pool is gone, and it lives as long as the guest.
        let _ = self.spawner.spawn_local(future);
    }

    fn poll_local(&self) {
        // A task that polls again from inside the pool finds it borrowed; the
        // outer run picks its work up.
        if let Ok(mut pool) = self.pool.try_borrow_mut() {
            pool.run_until_stalled();
        }
    }
}
