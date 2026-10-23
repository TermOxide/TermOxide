//! The app's state saved on disk, so it survives a restart of the host itself:
//! the same bytes the host keeps for a lost subprocess, fingerprint included,
//! so a snapshot saved by another state type is caught like any other state.

use std::{
    fs,
    io,
    path::{Path, PathBuf},
};

/// Turns persistence on (`1`, `true`) or off (`0`, `false`) for one run,
/// whatever `run_app!` says.
pub(crate) const PERSIST_ENV: &str = "TERMOXIDE_HOT_RELOAD_PERSIST";
/// Set to `1` or `true` to start from a clean state for one run: the snapshot
/// is ignored, then replaced by the run's own.
pub(crate) const FRESH_ENV: &str = "TERMOXIDE_HOT_RELOAD_FRESH";

/// Whether a run persists: `persistent`, as `run_app!` was called, unless
/// [`PERSIST_ENV`] is set to `setting`.
pub(crate) fn persists(setting: Option<&str>, persistent: bool) -> bool {
    match setting {
        Some("1" | "true") => true,
        Some("0" | "false") => false,
        _ => persistent,
    }
}

/// Whether [`FRESH_ENV`], set to `setting`, asks to start from a clean state.
pub(crate) fn is_fresh(setting: Option<&str>) -> bool { matches!(setting, Some("1" | "true")) }

/// Where the app's state is saved between runs: one fixed path per app, so a
/// new run finds what the last one wrote.
pub(crate) struct Snapshot {
    path: PathBuf,
}

impl Snapshot {
    /// `state.bin` in `dir`, the app's directory, next to its logs.
    pub(crate) fn in_dir(dir: &Path) -> Self { Self { path: dir.join("state.bin") } }

    pub(crate) fn path(&self) -> &Path { &self.path }

    /// The state an earlier run saved, if there is one and the run doesn't
    /// start `fresh`.
    pub(crate) fn load(&self, fresh: bool) -> io::Result<Option<Vec<u8>>> {
        if fresh {
            log::info!(
                "starting from a clean state as {FRESH_ENV} asks: {} is ignored",
                self.path.display()
            );
            return Ok(None);
        }
        match fs::read(&self.path) {
            Ok(state) => Ok(Some(state)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Saves `state` in place of the last: written beside it, then renamed
    /// over it, so a host killed mid-write leaves the previous snapshot whole.
    pub(crate) fn save(&self, state: &[u8]) -> io::Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let partial = self.path.with_extension("bin.partial");
        fs::write(&partial, state)?;
        fs::rename(&partial, &self.path)
    }

    /// Moves a snapshot that can't be read aside, as `state.bin.unreadable`,
    /// for inspection, and returns where it went.
    pub(crate) fn set_aside(&self) -> io::Result<PathBuf> {
        let aside = self.path.with_extension("bin.unreadable");
        fs::rename(&self.path, &aside)?;
        Ok(aside)
    }
}

#[cfg(test)]
mod tests;
