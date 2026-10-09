use std::{env, path::PathBuf};

use color_eyre::{Result, eyre::eyre};
use termoxide::App;

mod ipc;
mod logging;
mod subprocess;

use logging::{LOG_ENV, LogFile, Tag};
use subprocess::{PIPE_ENV, Subprocess};

/// Runs `app` with a hot-reload subprocess next to it.
///
/// The same executable is started again as the subprocess; there, this
/// function serves the host's requests instead of running `app`. Both log to
/// one file per run; install your own panic hook (`color_eyre::install()`, say)
/// before calling this.
pub async fn run_app<A: App + Clone + 'static>(app: A) -> Result<()> {
    if let Ok(pipe) = env::var(PIPE_ENV) {
        let log = env::var_os(LOG_ENV).ok_or_else(|| eyre!("{LOG_ENV} is not set"))?;
        logging::install(LogFile::open(PathBuf::from(log))?, Tag::Child)?;
        logging::log_panics(false);
        return Ok(subprocess::serve(&pipe)?);
    }

    let log = LogFile::create()?;
    let log_path = log.path().to_owned();
    logging::install(log, Tag::Host)?;
    logging::log_panics(true);

    let mut subprocess = Subprocess::spawn(&log_path)?;
    subprocess.ping()?;
    termoxide::run_with_app(app).await
}
