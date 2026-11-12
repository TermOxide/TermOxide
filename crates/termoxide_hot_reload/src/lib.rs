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
#[doc(hidden)]
pub mod cli;
#[cfg(feature = "host")]
mod component;
#[cfg(feature = "host")]
mod config;
#[cfg(feature = "host")]
mod dump;
#[cfg(feature = "guest")]
#[doc(hidden)]
pub mod guest;
mod ipc;
#[cfg(feature = "host")]
mod logging;
#[cfg(feature = "host")]
mod metadata;
#[cfg(feature = "host")]
mod native;
#[cfg(feature = "host")]
mod remote;
mod schema;
#[cfg(feature = "guest")]
mod session;
#[cfg(feature = "host")]
mod snapshot;
#[cfg(any(feature = "host", feature = "guest"))]
mod state;
#[cfg(feature = "host")]
mod subprocess;
#[cfg(feature = "host")]
mod watch;

#[cfg(feature = "host")]
#[doc(hidden)]
pub use build::Package;
#[doc(hidden)]
pub use termoxide_hot_reload_macros::app_lib as __app_lib;

/// What an app must be for its state to cross to the host and to its trace.
#[doc(hidden)]
pub trait HotApp: termoxide::App + Clone + serde::Serialize + serde::de::DeserializeOwned + 'static {}

impl<A: termoxide::App + Clone + serde::Serialize + serde::de::DeserializeOwned + 'static> HotApp for A {}

/// Exports `app`, the app as first built, as the hot-reload guest when the
/// crate is compiled for wasm; natively, it is what a release build of the
/// app's executable runs.
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

        #[cfg(not(target_arch = "wasm32"))]
        #[doc(hidden)]
        pub fn __termoxide_app() -> impl $crate::HotApp { $app }
    };
}

/// Runs the app of the calling crate with hot reload: builds it as a wasm
/// component, runs that in a subprocess, and drives it from this process, which
/// keeps the terminal and the run loop. Whenever the crate's `src` or manifest
/// changes, the app is rebuilt in the background and swapped in, keeping its
/// state.
///
/// The crate's lib must be both a `cdylib` and an `rlib`, with a `guest`
/// feature that enables this crate's, and declare its app with [`wasm_app!`].
/// Both processes log to one file per run; install your own panic hook
/// (`color_eyre::install()`, say) before calling this.
///
/// `run_app!(persistent)` also keeps the app's state across runs: each run
/// traces every change of a recorded signal to `state.bin`, next to the logs,
/// and the next run starts from its last entry. `persist = true` or `false` in
/// `config.toml`, next to the logs, overrides that for every run, and
/// `TERMOXIDE_PERSIST=0` or `1` for one run. `TERMOXIDE_FRESH=1` starts that
/// run from a clean state. When the app stops for crashing too often, the
/// trace is kept as `state.bin.crashed`. `termoxide dump` prints a trace's
/// entries.
///
/// A release build, any profile without debug assertions, runs the app's lib
/// linked into the executable instead: no wasm, no subprocess, no hot reload.
/// It keeps the log file and the trace, which `termoxide dump` reads, and a
/// dev run can start from.
#[cfg(feature = "host")]
#[macro_export]
macro_rules! run_app {
    () => {
        $crate::__run_app!(false)
    };
    (persistent) => {
        $crate::__run_app!(true)
    };
}

/// [`run_app!`], `persistent` or not. The profile is the app's, as the `cfg`
/// is read where this expands.
#[cfg(feature = "host")]
#[macro_export]
#[doc(hidden)]
macro_rules! __run_app {
    ($persistent:literal) => {{
        let package = $crate::Package { manifest_dir: env!("CARGO_MANIFEST_DIR"), app: None };
        #[cfg(debug_assertions)]
        let run = $crate::run(package, $persistent);
        #[cfg(not(debug_assertions))]
        let run = $crate::run_native($crate::__app_lib!(), package, $persistent);
        run
    }};
}

/// What a dev build's [`run_app!`] runs.
#[cfg(feature = "host")]
#[doc(hidden)]
pub async fn run(package: Package, persistent: bool) -> color_eyre::Result<()> {
    if is_child() {
        return serve();
    }
    let persists = persists(&package.app_name()?, persistent)?;
    let fresh = snapshot::is_fresh(std::env::var(snapshot::FRESH_ENV).ok().as_deref());
    host(package, persists, fresh).await
}

/// What a release build's [`run_app!`] runs: the app `build` makes, from the
/// lib named `lib`.
#[cfg(feature = "host")]
#[doc(hidden)]
pub async fn run_native<A: HotApp>(
    (build, lib): (impl FnOnce() -> A, &'static str),
    package: Package,
    persistent: bool,
) -> color_eyre::Result<()> {
    use logging::LogFile;
    use snapshot::Snapshot;

    let app = package.app_name()?;
    let persists = persists(&app, persistent)?;
    let fresh = snapshot::is_fresh(std::env::var(snapshot::FRESH_ENV).ok().as_deref());
    logging::install_native(LogFile::create(&app)?, lib)?;
    logging::log_panics(true);

    // Building or decoding the app makes signals, which need an owner.
    let owner = termoxide_reactive::Owner::new();
    owner.set();
    if !persists {
        return termoxide::run_with_app(build()).await;
    }
    let snapshot = Snapshot::in_dir(&logging::app_dir(&app)?, package.manifest_dir);
    log::info!("keeping the app's state in {}", snapshot.path().display());
    termoxide::run_with_app(native::Persisting::start(build, snapshot, fresh)?).await
}

/// Whether a run of `app` persists: the environment, then the app's config,
/// then `persistent`, its default.
#[cfg(feature = "host")]
fn persists(app: &str, persistent: bool) -> color_eyre::Result<bool> {
    let config = config::Config::read(&logging::app_dir(app)?)?;
    let setting = std::env::var(snapshot::PERSIST_ENV).ok();
    Ok(snapshot::persists(setting.as_deref(), config.persist, persistent))
}

/// Whether this process is the subprocess a host spawned, from its own
/// executable, to run the app's component.
#[cfg(feature = "host")]
fn is_child() -> bool { std::env::var_os(subprocess::PIPE_ENV).is_some() }

/// Runs the app's component for the host that spawned this process.
#[cfg(feature = "host")]
fn serve() -> color_eyre::Result<()> {
    use std::{env, path::PathBuf};

    use color_eyre::eyre::eyre;
    use logging::{LOG_ENV, LogFile, Tag};
    use subprocess::{GUEST_ENV, PIPE_ENV};

    let pipe = env::var(PIPE_ENV)?;
    let log = env::var_os(LOG_ENV).ok_or_else(|| eyre!("{LOG_ENV} is not set"))?;
    logging::install(LogFile::open(PathBuf::from(log))?, Tag::Child)?;
    logging::log_panics(false);
    let guest = PathBuf::from(env::var_os(GUEST_ENV).ok_or_else(|| eyre!("{GUEST_ENV} is not set"))?);
    // On a thread of its own, outside the app's async runtime: wasmtime
    // runs the component's stdio on a runtime of its own, and one can't
    // start inside another.
    std::thread::spawn(move || subprocess::serve(&pipe, &guest))
        .join()
        .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
}

/// Builds the app of `package`, then runs and hot reloads it, keeping its
/// state across runs when it `persists`, from a clean one when `fresh`.
#[cfg(feature = "host")]
async fn host(package: Package, persists: bool, fresh: bool) -> color_eyre::Result<()> {
    use logging::{LogFile, Tag};
    use remote::RemoteApp;
    use snapshot::Snapshot;

    // Before the log and the terminal, so cargo's errors land on the screen.
    let guest = build::guest(package)?;
    let rebuilds = watch::Rebuilds::start(package)?;

    let app = package.app_name()?;
    let log = LogFile::create(&app)?;
    let log_path = log.path().to_owned();
    logging::install(log, Tag::Host)?;
    logging::log_panics(true);

    let snapshot = persists
        .then(|| logging::app_dir(&app).map(|dir| Snapshot::in_dir(&dir, package.manifest_dir)))
        .transpose()?;
    if let Some(snapshot) = &snapshot {
        log::info!("keeping the app's state in {}", snapshot.path().display());
    }
    termoxide::run_with_app(RemoteApp::start(rebuilds, guest, log_path, snapshot, fresh)?).await
}
