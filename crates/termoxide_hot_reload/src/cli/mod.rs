//! What the `termoxide` command does. `termoxide_cli` reads its arguments.

use std::{env, path::Path};

use color_eyre::Result;

use crate::{
    build::Package,
    dump,
    metadata::AppCrate,
    snapshot::{self, FRESH_ENV},
};
pub use crate::{dump::Dump, snapshot::Aside};

/// Whether this process is the subprocess of a `termoxide run`, which is this
/// same executable started with no arguments.
pub fn is_child() -> bool { crate::is_child() }

/// Runs the app's component for the `termoxide run` that started this process.
pub fn serve() -> Result<()> { crate::serve() }

/// Hosts the app of the crate at `path`, as `run_app!` in its own executable
/// would. `persist`, when given, comes before the environment, the app's
/// config and its own default; `fresh` starts it from a clean state.
pub async fn run(path: &Path, persist: Option<bool>, fresh: bool) -> Result<()> {
    let app = AppCrate::read(path)?;
    app.check_runnable()?;
    // Read even when `persist` is given, so a broken config is never missed.
    let persists = persist.unwrap_or(crate::persists(&app.name, app.persistent())?);
    let fresh = fresh || snapshot::is_fresh(env::var(FRESH_ENV).ok().as_deref());
    // Leaked once, as `env!` gives them to `run_app!`.
    let package = Package {
        manifest_dir: Box::leak(app.dir().to_string_lossy().into_owned().into_boxed_str()),
        app: Some(Box::leak(app.name.into_boxed_str())),
    };
    crate::host(package, persists, fresh).await
}

/// Prints the state trace `dump` names.
pub fn dump(dump: &Dump) -> Result<()> { dump::print_trace(dump) }
