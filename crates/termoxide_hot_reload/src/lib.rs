//! Hot reload for TermOxide apps.
//!
//! Two features, one per side: `host` for the native executable (the process
//! that owns the terminal, and the subprocess it spawns to run the app's wasm
//! component), `guest` for the app built as that component. What neither gates
//! is the protocol they share.

// Without a side, only the shared protocol compiles, and nothing uses it.
#![cfg_attr(not(any(feature = "host", feature = "guest")), allow(dead_code, unused_imports))]

// The guest's generated bindings name this crate by path, from the app's crate
// and from this one alike.
#[cfg(feature = "guest")]
extern crate self as termoxide_hot_reload;

#[cfg(feature = "host")]
mod build;
#[cfg(feature = "host")]
mod component;
#[cfg(feature = "guest")]
#[doc(hidden)]
pub mod guest;
mod ipc;
#[cfg(feature = "host")]
mod logging;
#[cfg(feature = "host")]
mod remote;
mod schema;
#[cfg(feature = "guest")]
mod session;
#[cfg(feature = "host")]
mod snapshot;
#[cfg(feature = "host")]
mod subprocess;
#[cfg(feature = "host")]
mod watch;

#[cfg(feature = "host")]
#[doc(hidden)]
pub use build::Package;

/// Exports `app`, the app as first built, as the hot-reload guest when the
/// crate is compiled for wasm; expands to nothing otherwise.
///
/// ```ignore
/// termoxide_hot_reload::wasm_app!(Counter::default());
/// ```
#[macro_export]
macro_rules! wasm_app {
    ($app:expr) => {
        #[cfg(target_arch = "wasm32")]
        const _: () = {
            struct Guest;

            impl $crate::guest::bindings::Guest for Guest {
                fn wire_schema() -> u64 { $crate::guest::wire_schema() }

                fn handle(
                    request: ::std::vec::Vec<u8>,
                ) -> ::std::result::Result<::std::vec::Vec<u8>, ::std::string::String> {
                    $crate::guest::handle(request, || $app)
                }
            }

            $crate::guest::bindings::export_guest!(Guest);
        };
    };
}

/// Runs the app of the calling crate with hot reload: builds it as a wasm
/// component, runs that in a subprocess, and drives it from this process, which
/// keeps the terminal and the run loop. Whenever the crate's `src` or manifest
/// changes, the app is rebuilt in the background and swapped in, keeping its
/// state.
///
/// The crate's lib must be a `cdylib` with a `guest` feature that enables this
/// crate's, and declare its app with [`wasm_app!`]. Both processes log to one
/// file per run; install your own panic hook (`color_eyre::install()`, say)
/// before calling this.
///
/// `run_app!(persistent)` also keeps the app's state across runs, in a file
/// next to the logs. `TERMOXIDE_HOT_RELOAD_PERSIST=0` or `1` overrides that
/// for one run, and `TERMOXIDE_HOT_RELOAD_FRESH=1` starts that run from a clean
/// state.
#[cfg(feature = "host")]
#[macro_export]
macro_rules! run_app {
    () => {
        $crate::run($crate::Package { manifest_dir: env!("CARGO_MANIFEST_DIR") }, false)
    };
    (persistent) => {
        $crate::run($crate::Package { manifest_dir: env!("CARGO_MANIFEST_DIR") }, true)
    };
}

/// What [`run_app!`] expands to.
#[cfg(feature = "host")]
#[doc(hidden)]
pub async fn run(package: Package, persistent: bool) -> color_eyre::Result<()> {
    use std::{env, path::PathBuf};

    use color_eyre::eyre::eyre;
    use logging::{LOG_ENV, LogFile, Tag};
    use remote::RemoteApp;
    use snapshot::{FRESH_ENV, PERSIST_ENV, Snapshot};
    use subprocess::{GUEST_ENV, PIPE_ENV};

    if let Ok(pipe) = env::var(PIPE_ENV) {
        let log = env::var_os(LOG_ENV).ok_or_else(|| eyre!("{LOG_ENV} is not set"))?;
        logging::install(LogFile::open(PathBuf::from(log))?, Tag::Child)?;
        logging::log_panics(false);
        let guest = PathBuf::from(env::var_os(GUEST_ENV).ok_or_else(|| eyre!("{GUEST_ENV} is not set"))?);
        // On a thread of its own, outside the app's async runtime: wasmtime
        // runs the component's stdio on a runtime of its own, and one can't
        // start inside another.
        return std::thread::spawn(move || subprocess::serve(&pipe, &guest))
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
    }

    // Before the log and the terminal, so cargo's errors land on the screen.
    let guest = build::guest(package)?;
    let rebuilds = watch::Rebuilds::start(package)?;

    let log = LogFile::create()?;
    let log_path = log.path().to_owned();
    logging::install(log, Tag::Host)?;
    logging::log_panics(true);

    let persists = snapshot::persists(env::var(PERSIST_ENV).ok().as_deref(), persistent);
    let snapshot = persists
        .then(|| logging::app_dir().map(|dir| Snapshot::in_dir(&dir)))
        .transpose()?;
    if let Some(snapshot) = &snapshot {
        log::info!("keeping the app's state in {}", snapshot.path().display());
    }
    let fresh = snapshot::is_fresh(env::var(FRESH_ENV).ok().as_deref());
    termoxide::run_with_app(RemoteApp::start(rebuilds, guest, log_path, snapshot, fresh)?).await
}
