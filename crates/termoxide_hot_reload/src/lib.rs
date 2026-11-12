use color_eyre::Result;
use termoxide::App;

mod ipc;
mod subprocess;

use subprocess::{PIPE_ENV, Subprocess};

/// Runs `app` with a hot-reload subprocess next to it.
///
/// The same executable is started again as the subprocess; there, this
/// function serves the host's requests instead of running `app`.
pub async fn run_app<A: App + Clone + 'static>(app: A) -> Result<()> {
    if let Ok(pipe) = std::env::var(PIPE_ENV) {
        return Ok(subprocess::serve(&pipe)?);
    }

    let mut subprocess = Subprocess::spawn()?;
    subprocess.ping()?;
    termoxide::run_with_app(app).await
}
