//! The app's state saved on disk, so it survives a restart of the host itself:
//! a trace of the bytes the host keeps for a lost subprocess, fingerprint
//! included, one entry per recorded change. A trace saved by another state type
//! is caught like any other state, and `termoxide_dump` prints one back.

use std::{
    cell::RefCell,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

/// Turns persistence on (`1`, `true`) or off (`0`, `false`) for one run,
/// whatever `run_app!` says.
pub(crate) const PERSIST_ENV: &str = "TERMOXIDE_HOT_RELOAD_PERSIST";
/// Set to `1` or `true` to start from a clean state for one run: the trace is
/// ignored, then replaced by the run's own.
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

/// What a trace starts with, readable in a text viewer, so a file that isn't
/// one, or is one of another layout, is told apart. Bumped with the layout.
const MAGIC: &[u8] = b"termoxide state trace 1
";

/// What follows [`MAGIC`]: whose trace it is.
#[derive(Serialize, Deserialize)]
struct Header<'a> {
    /// The app's crate: what builds the component that can decode the trace.
    manifest_dir: &'a str,
}

/// One recorded state.
#[derive(Serialize, Deserialize)]
pub(crate) struct Entry<'a> {
    /// When, in milliseconds since the Unix epoch.
    pub(crate) at: u64,
    #[serde(borrow)]
    pub(crate) state: &'a [u8],
}

/// A trace, as read from its bytes.
pub(crate) struct Trace<'a> {
    pub(crate) manifest_dir: &'a str,
    /// Its complete entries, in order.
    pub(crate) entries: Vec<Entry<'a>>,
    /// More bytes follow the entries: a last one cut short by a host killed
    /// while writing it.
    pub(crate) cut_short: bool,
}

/// `bytes` as a trace, or `None` when they don't start like one.
pub(crate) fn parse(bytes: &[u8]) -> Option<Trace<'_>> {
    let (header, mut rest) = postcard::take_from_bytes::<Header<'_>>(bytes.strip_prefix(MAGIC)?).ok()?;
    let mut entries = Vec::new();
    while let Ok((entry, after)) = postcard::take_from_bytes::<Entry<'_>>(rest) {
        entries.push(entry);
        rest = after;
    }
    Some(Trace { manifest_dir: header.manifest_dir, entries, cut_short: !rest.is_empty() })
}

/// Where the app's state is traced between runs: one fixed path per app, so a
/// new run finds what the last one wrote.
pub(crate) struct Snapshot {
    path: PathBuf,
    /// The app's crate, named in the trace's header.
    manifest_dir: &'static str,
    /// This run's trace, open for appending once its first entry is written.
    trace: RefCell<Option<File>>,
}

impl Snapshot {
    /// `state.bin` in `dir`, the app's directory, next to its logs, for the
    /// app whose crate is at `manifest_dir`.
    pub(crate) fn in_dir(dir: &Path, manifest_dir: &'static str) -> Self {
        Self { path: dir.join("state.bin"), manifest_dir, trace: RefCell::new(None) }
    }

    pub(crate) fn path(&self) -> &Path { &self.path }

    /// The last state an earlier run recorded, if there is one and the run
    /// doesn't start `fresh`. A file that isn't a trace, or a trace without a
    /// complete entry, is an [`InvalidData`](io::ErrorKind::InvalidData) error.
    pub(crate) fn load(&self, fresh: bool) -> io::Result<Option<Vec<u8>>> {
        if fresh {
            log::info!(
                "starting from a clean state as {FRESH_ENV} asks: {} is ignored",
                self.path.display()
            );
            return Ok(None);
        }
        let trace = match fs::read(&self.path) {
            Ok(trace) => trace,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        let Some(trace) = parse(&trace) else {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "the snapshot isn't a state trace"));
        };
        match trace.entries.last() {
            Some(entry) => Ok(Some(entry.state.to_vec())),
            None => Err(io::Error::new(io::ErrorKind::InvalidData, "the trace holds no complete entry")),
        }
    }

    /// Adds `state` to this run's trace. The first entry starts it in place of
    /// the last run's: written beside it, then renamed over it, so a host
    /// killed meanwhile leaves the previous trace whole. One killed while
    /// appending leaves the last entry cut short, which reading skips.
    pub(crate) fn record(&self, state: &[u8]) -> io::Result<()> {
        let at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|since| u64::try_from(since.as_millis()).unwrap_or(u64::MAX))
            .unwrap_or_default();
        let entry = postcard::to_stdvec(&Entry { at, state }).map_err(io::Error::other)?;
        let mut trace = self.trace.borrow_mut();
        if let Some(file) = trace.as_mut() {
            return file.write_all(&entry);
        }
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let header = postcard::to_stdvec(&Header { manifest_dir: self.manifest_dir }).map_err(io::Error::other)?;
        let partial = self.path.with_extension("bin.partial");
        fs::write(&partial, [MAGIC, &header, &entry].concat())?;
        fs::rename(&partial, &self.path)?;
        *trace = Some(OpenOptions::new().append(true).open(&self.path)?);
        Ok(())
    }

    /// Moves the trace aside, for reading, and returns where it went. The next
    /// entry starts a new one.
    pub(crate) fn set_aside(&self, why: Aside) -> io::Result<PathBuf> {
        // Windows doesn't rename a file that is open.
        self.trace.borrow_mut().take();
        let aside = self.path.with_extension(match why {
            Aside::Unreadable => "bin.unreadable",
            Aside::Crashed => "bin.crashed",
        });
        fs::rename(&self.path, &aside)?;
        Ok(aside)
    }
}

/// Why a trace is set aside.
pub(crate) enum Aside {
    /// Its last state doesn't decode.
    Unreadable,
    /// The app kept crashing from its latest state.
    Crashed,
}

#[cfg(test)]
mod tests;
