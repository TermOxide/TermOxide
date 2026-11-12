use std::path::Path;

use color_eyre::{Result, eyre::eyre};
use wasmtime::{
    Engine,
    Store,
    component::{Component, Linker, ResourceTable},
};
use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView};

use crate::ipc::{Request, Response};

wasmtime::component::bindgen!({ world: "guest", path: "wit" });

/// The app's wasm component, loaded in the subprocess.
pub(crate) struct GuestInstance {
    store: Store<State>,
    guest: Guest,
}

/// What the component's WASI imports run against. Its stdio is not wired: an
/// app logs through the framework's logger instead.
struct State {
    ctx: WasiCtx,
    table: ResourceTable,
}

impl WasiView for State {
    fn ctx(&mut self) -> WasiCtxView<'_> { WasiCtxView { ctx: &mut self.ctx, table: &mut self.table } }
}

impl GuestInstance {
    /// Compiles and instantiates the component at `path`.
    pub(crate) fn load(path: &Path) -> Result<Self> {
        let engine = Engine::default();
        let component = Component::from_file(&engine, path).map_err(|e| eyre!("loading {}: {e:#}", path.display()))?;
        let mut linker = Linker::new(&engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker).map_err(|e| eyre!("{e:#}"))?;
        let mut store = Store::new(&engine, State { ctx: WasiCtx::builder().build(), table: ResourceTable::new() });
        let guest = Guest::instantiate(&mut store, &component, &linker).map_err(|e| eyre!("{e:#}"))?;
        Ok(Self { store, guest })
    }

    /// Hands `request` to the app and returns its answer.
    pub(crate) fn handle(&mut self, request: &Request) -> Result<Response> {
        let request = postcard::to_allocvec(request)?;
        let response = self
            .guest
            .call_handle(&mut self.store, &request)
            .map_err(|e| eyre!("the guest trapped: {e:#}"))?
            .map_err(|e| eyre!("the guest refused a request: {e}"))?;
        Ok(postcard::from_bytes(&response)?)
    }
}
