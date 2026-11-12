use std::{
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};

use color_eyre::{Result, eyre::eyre};
use wasmtime::{
    Cache,
    Config,
    Engine,
    Store,
    component::{Component, HasSelf, Linker, ResourceTable},
};
use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView};

use crate::{
    ipc::{Request, Response},
    logging::{APP_TARGET, AppOutput},
};

wasmtime::component::bindgen!({ world: "guest", path: "wit" });

/// How often the engine's epoch advances: how finely a call's budget is
/// measured.
const EPOCH_TICK: Duration = Duration::from_millis(10);
/// How long one guest call may run before it is stopped: under the host's
/// one-second wait for an answer, so the subprocess still has time to restart
/// the app and reply.
const CALL_BUDGET: Duration = Duration::from_millis(500);
/// [`CALL_BUDGET`] in epoch ticks, which is how wasmtime counts it.
const CALL_BUDGET_TICKS: u64 = (CALL_BUDGET.as_millis() / EPOCH_TICK.as_millis()) as u64;

/// What the subprocess runs the app's components with: one engine and one
/// linker for every generation.
pub(crate) struct Runtime {
    engine: Engine,
    linker: Linker<State>,
}

/// One generation of the app: an instance of its component.
pub(crate) struct GuestInstance {
    store: Store<State>,
    guest: Guest,
}

/// What the component's imports run against: its log records and its stdout and
/// stderr all go to the log file, as the app's lines.
struct State {
    ctx: WasiCtx,
    table: ResourceTable,
}

impl WasiView for State {
    fn ctx(&mut self) -> WasiCtxView<'_> { WasiCtxView { ctx: &mut self.ctx, table: &mut self.table } }
}

impl GuestImports for State {
    fn log(&mut self, level: Level, message: String) {
        let level = match level {
            Level::Error => log::Level::Error,
            Level::Warn => log::Level::Warn,
            Level::Info => log::Level::Info,
            Level::Debug => log::Level::Debug,
            Level::Trace => log::Level::Trace,
        };
        log::log!(target: APP_TARGET, level, "{message}");
    }
}

impl Runtime {
    pub(crate) fn new() -> Result<Self> {
        let mut config = Config::new();
        // A runaway call, even a bare `loop {}`, traps once its budget runs out,
        // with no cooperation from the app's code.
        config.epoch_interruption(true);
        // Compiling a component takes seconds; the cache makes an unchanged one
        // load fast across runs. Without it, every load just compiles.
        match Cache::from_file(None) {
            Ok(cache) => {
                config.cache(Some(cache));
            },
            Err(e) => log::warn!("no compilation cache: {e:#}"),
        }
        let engine = Engine::new(&config).map_err(|e| eyre!("{e:#}"))?;
        thread::spawn({
            let engine = engine.clone();
            move || {
                loop {
                    thread::sleep(EPOCH_TICK);
                    engine.increment_epoch();
                }
            }
        });
        let mut linker = Linker::new(&engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker).map_err(|e| eyre!("{e:#}"))?;
        Guest::add_to_linker::<_, HasSelf<_>>(&mut linker, |state| state).map_err(|e| eyre!("{e:#}"))?;
        Ok(Self { engine, linker })
    }

    /// Compiles the component at `path`.
    pub(crate) fn compile(&self, path: &Path) -> Result<Component> {
        Component::from_file(&self.engine, path).map_err(|e| eyre!("compiling {}: {e:#}", path.display()))
    }

    /// Compiles the component at `path` on a thread of its own, so the running
    /// generation keeps answering meanwhile.
    pub(crate) fn compile_in_background(&self, path: PathBuf) -> Receiver<Result<Component>> {
        let engine = self.engine.clone();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let component =
                Component::from_file(&engine, &path).map_err(|e| eyre!("compiling {}: {e:#}", path.display()));
            let _ = tx.send(component);
        });
        rx
    }

    /// Starts a new generation of `component`.
    pub(crate) fn instantiate(&self, component: &Component) -> Result<GuestInstance> {
        // Stdout at `info` and stderr at `debug`, as the subprocess's own.
        let ctx = WasiCtx::builder()
            .stdout(AppOutput::new(log::Level::Info))
            .stderr(AppOutput::new(log::Level::Debug))
            .build();
        let mut store = Store::new(&self.engine, State { ctx, table: ResourceTable::new() });
        store.epoch_deadline_trap();
        // Instantiating runs the component's own start-up code: it needs a
        // budget too, or the store's initial deadline of 0 stops it at once.
        store.set_epoch_deadline(CALL_BUDGET_TICKS);
        let guest = Guest::instantiate(&mut store, component, &self.linker).map_err(|e| eyre!("{e:#}"))?;
        Ok(GuestInstance { store, guest })
    }
}

impl GuestInstance {
    /// The fingerprint of the protocol the component was built with.
    pub(crate) fn wire_schema(&mut self) -> Result<u64> {
        self.store.set_epoch_deadline(CALL_BUDGET_TICKS);
        self.guest
            .call_wire_schema(&mut self.store)
            .map_err(|e| eyre!("the guest trapped: {e:#}"))
    }

    /// Hands `request` to the app and returns its answer.
    pub(crate) fn handle(&mut self, request: &Request) -> Result<Response> {
        let request = postcard::to_allocvec(request)?;
        self.store.set_epoch_deadline(CALL_BUDGET_TICKS);
        let response = self
            .guest
            .call_handle(&mut self.store, &request)
            .map_err(|e| eyre!("the guest trapped: {e:#}"))?
            .map_err(|e| eyre!("the guest refused a request: {e}"))?;
        Ok(postcard::from_bytes(&response)?)
    }
}
