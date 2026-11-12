//! What the `termoxide` command does. `termoxide_cli` reads its arguments.

use std::{env, path::Path, process::Command};

use color_eyre::Result;

use crate::{
    build::Package,
    dump,
    metadata::AppCrate,
    runner,
    snapshot::{self, FRESH_ENV, PERSIST_ENV},
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

/// Builds the app of the crate at `path` for release, linked into an
/// executable of its own, and runs that in place of this process. `persist`
/// and `fresh` are as [`run`]'s.
pub fn run_release(path: &Path, persist: Option<bool>, fresh: bool) -> Result<()> {
    let app = AppCrate::read(path)?;
    app.check_runnable()?;
    let mut command = Command::new(runner::build(&app)?);
    if let Some(persist) = persist {
        command.env(PERSIST_ENV, if persist { "1" } else { "0" });
    }
    if fresh {
        command.env(FRESH_ENV, "1");
    }
    exec(command)
}

#[cfg(unix)]
fn exec(mut command: Command) -> Result<()> {
    use std::os::unix::process::CommandExt;

    Err(command.exec().into())
}

/// Windows can't replace a process, so this one waits for the app's and exits
/// as it did.
#[cfg(not(unix))]
fn exec(mut command: Command) -> Result<()> {
    let status = command.status()?;
    std::process::exit(status.code().unwrap_or(1))
}

/// Prints the state trace `dump` names.
pub fn dump(dump: &Dump) -> Result<()> { dump::print_trace(dump) }
