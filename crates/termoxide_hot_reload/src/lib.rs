//! Hot reload for TermOxide apps.
//!
//! Two features, one per side: `host` for the native processes (the one that
//! owns the terminal and the subprocess it talks to), `guest` for what runs the
//! app. What neither gates is the protocol they share.

// Until the app runs as a wasm guest, only the two features together have an
// entry point (`run_app`), so either alone leaves code unused.
#![cfg_attr(not(all(feature = "host", feature = "guest")), allow(dead_code, unused_imports))]

// The guest's generated bindings name this crate by path, from the app's crate
// and from this one alike.
#[cfg(feature = "guest")]
extern crate self as termoxide_hot_reload;

#[cfg(feature = "guest")]
#[doc(hidden)]
pub mod guest;
mod ipc;
#[cfg(feature = "host")]
mod logging;
#[cfg(feature = "host")]
mod remote;
#[cfg(feature = "guest")]
mod session;
#[cfg(feature = "host")]
mod subprocess;

/// Runs `app` in a hot-reload subprocess, driven from this process.
///
/// The same executable is started again as the subprocess, and `app` runs
/// there: this process keeps the terminal and the run loop, and forwards every
/// call to it. The app crosses as bytes, so its type must (de)serialize: derive
/// both on a struct of signals. Both processes log to one file per run; install
/// your own panic hook (`color_eyre::install()`, say) before calling this.
///
/// Needs both features: until the app runs as a wasm guest, the subprocess runs
/// it natively, so one executable is both sides.
#[cfg(all(feature = "host", feature = "guest"))]
pub async fn run_app<A>(app: A) -> color_eyre::Result<()>
where
    A: termoxide::App + Clone + serde::Serialize + serde::de::DeserializeOwned + 'static,
{
    use std::{env, path::PathBuf};

    use color_eyre::eyre::eyre;
    use logging::{LOG_ENV, LogFile, Tag};
    use remote::RemoteApp;
    use subprocess::{PIPE_ENV, Subprocess};

    if let Ok(pipe) = env::var(PIPE_ENV) {
        let log = env::var_os(LOG_ENV).ok_or_else(|| eyre!("{LOG_ENV} is not set"))?;
        logging::install(LogFile::open(PathBuf::from(log))?, Tag::Child)?;
        logging::log_panics(false);
        return Ok(subprocess::serve(&pipe, app).await?);
    }

    let log = LogFile::create()?;
    let log_path = log.path().to_owned();
    logging::install(log, Tag::Host)?;
    logging::log_panics(true);

    let mut subprocess = Subprocess::spawn(&log_path)?;
    subprocess.ping()?;
    // This process never runs `app` itself: the subprocess does.
    drop(app);
    termoxide::run_with_app(RemoteApp::start(subprocess)?).await
}
