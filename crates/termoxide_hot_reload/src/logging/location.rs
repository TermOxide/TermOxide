use std::{env, io, path::PathBuf, process};

use dirs::{data_local_dir, state_dir};
use time::{OffsetDateTime, macros::format_description};

/// Set on the subprocess to the path of the log file it must append to.
pub(crate) const LOG_ENV: &str = "TERMOXIDE_HOT_RELOAD_LOG";

/// `termoxide/<app>/logs` in the per-user state directory, or in the local
/// data directory where there is none:
/// - Windows: `%LOCALAPPDATA%\termoxide\<app>\logs`
/// - Linux: `$XDG_STATE_HOME/termoxide/<app>/logs`, by default under `~/.local/state`
/// - macOS: `~/Library/Application Support/termoxide/<app>/logs`
pub(super) fn log_dir() -> io::Result<PathBuf> {
    let base = state_dir()
        .or_else(data_local_dir)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no per-user directory to put logs in"))?;
    Ok(base.join("termoxide").join(app_name()?).join("logs"))
}

/// The executable's file stem, standing in for the app's name.
fn app_name() -> io::Result<String> {
    let exe = env::current_exe()?;
    let stem = exe
        .file_stem()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "the executable has no file name"))?;
    Ok(stem.to_string_lossy().into_owned())
}

/// `started` as a file name, without the colons Windows rejects. The pid
/// tells apart two runs started in the same second.
pub(super) fn file_name(started: OffsetDateTime) -> String {
    let stamp = started
        .format(format_description!("[year]-[month]-[day]_[hour]-[minute]-[second]"))
        .unwrap_or_default();
    format!("{stamp}_{}.log", process::id())
}
