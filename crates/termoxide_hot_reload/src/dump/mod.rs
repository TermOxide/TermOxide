//! `termoxide dump`: prints a state trace a persistent run left, each entry
//! with when it was recorded, as the app that wrote it reads it. Only the app's
//! own component can decode its state, so this builds it from the app's crate.

use std::{
    fs,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
};

use color_eyre::{Result, eyre::eyre};
use time::{OffsetDateTime, macros::format_description};

use crate::{
    build::{self, Package},
    component::Runtime,
    ipc::{Request, Response},
    logging,
    metadata::AppCrate,
    schema,
    snapshot::{self, Aside, Entry},
};

/// ANSI bold yellow, for a state type that changed since the previous entry.
const CHANGED: &str = "\x1b[1;33m";
/// ANSI back to the terminal's own style.
const RESET: &str = "\x1b[0m";

/// What `termoxide dump` prints.
#[derive(Debug)]
pub struct Dump {
    /// A trace, or the crate folder of the app whose trace it is.
    pub path: PathBuf,
    /// Which binary of the app's crate wrote the trace.
    pub bin: Option<String>,
    /// The trace set aside, rather than the one the next run starts from.
    pub aside: Option<Aside>,
    /// The app's crate, its folder or its manifest, when it isn't where the
    /// trace says: it moved.
    pub manifest: Option<PathBuf>,
}

impl Dump {
    /// The trace to print, and the crate folder it was found through.
    fn trace(&self) -> Result<(PathBuf, Option<PathBuf>)> {
        if !self.path.is_dir() {
            if self.bin.is_some() || self.aside.is_some() {
                return Err(eyre!(
                    "--bin, --crashed and --unreadable pick the trace of an app's crate folder, and {} is a file",
                    self.path.display()
                ));
            }
            return Ok((self.path.clone(), None));
        }
        let app = AppCrate::read(&self.path)?;
        let current = logging::app_dir(&app.app_name(self.bin.as_deref())?)?.join(snapshot::FILE);
        let trace = self
            .aside
            .map_or_else(|| current.clone(), |why| current.with_extension(why.extension()));
        if !trace.exists() {
            let set_aside = [Aside::Crashed, Aside::Unreadable]
                .map(|why| current.with_extension(why.extension()))
                .into_iter()
                .find(|aside| aside.exists());
            return Err(match (self.aside, set_aside) {
                (None, Some(aside)) => {
                    eyre!(
                        "{} has no trace, but one was set aside as {}: pass --crashed or --unreadable",
                        app.name,
                        aside.display()
                    )
                },
                _ => eyre!("{} has no trace at {}", app.name, trace.display()),
            });
        }
        Ok((trace, Some(app.dir().to_owned())))
    }
}

/// The crate folder of `manifest`, given as the folder or its `Cargo.toml`.
fn manifest_dir(manifest: &Path) -> PathBuf {
    match manifest.file_name() {
        Some(name) if name == "Cargo.toml" => manifest.parent().map(Path::to_owned).unwrap_or_default(),
        _ => manifest.to_owned(),
    }
}

/// Prints the trace `dump` names.
pub(crate) fn print_trace(dump: &Dump) -> Result<()> {
    let (path, crate_dir) = dump.trace()?;
    let bytes = fs::read(&path)?;
    let trace = snapshot::parse(&bytes).ok_or_else(|| eyre!("{} isn't a state trace", path.display()))?;

    let manifest_dir = match dump.manifest.as_deref().map(manifest_dir).or(crate_dir) {
        Some(dir) => dir.to_string_lossy().into_owned(),
        None => trace.manifest_dir.to_owned(),
    };
    // A `Package` holds what `env!` gives `run_app!`: leaked once, for the
    // one build this process makes.
    let package = Package { manifest_dir: Box::leak(manifest_dir.into_boxed_str()), app: None };
    let guest = build::guest(package)?;

    let color = io::stdout().is_terminal();
    print(&trace.entries, trace.cut_short, &guest, color, &mut io::stdout().lock())
}

/// Prints `entries` to `out` as the component at `guest` decodes them, with
/// when each was recorded (UTC) and its state type's fingerprint, in `color`
/// where it changed.
fn print(entries: &[Entry<'_>], cut_short: bool, guest: &Path, color: bool, out: &mut impl Write) -> Result<()> {
    let runtime = Runtime::new()?;
    let component = runtime.compile(guest)?;
    let mut instance = None;
    let mut previous = None;
    for entry in entries {
        let at = OffsetDateTime::from_unix_timestamp_nanos(i128::from(entry.at) * 1_000_000)?.format(
            format_description!("[year]-[month]-[day] [hour]:[minute]:[second].[subsecond digits:3]"),
        )?;
        let generation = match &mut instance {
            Some(generation) => generation,
            None => {
                let mut generation = runtime.instantiate(&component)?;
                if generation.wire_schema()? != schema::wire().map_err(|e| eyre!(e))? {
                    return Err(eyre!("the app was built for another version of termoxide_hot_reload"));
                }
                instance.insert(generation)
            },
        };
        let (fingerprint, app) = match generation.handle(&Request::Describe { state: entry.state.to_vec() }) {
            Ok(Response::Described { fingerprint, app: Some(app) }) => (fingerprint, app),
            Ok(Response::Described { fingerprint, app: None }) => {
                (fingerprint, String::from("(saved by another state type)"))
            },
            Ok(_) => (None, String::from("(the app answered something else)")),
            Err(e) => {
                // A guest that trapped can't be called again.
                instance = None;
                (None, format!("(can't be read: {e:#})"))
            },
        };
        let changed = previous.is_some_and(|previous| previous != fingerprint);
        previous = Some(fingerprint);
        writeln!(out, "{at}  state type {}\n{app}\n", state_type(fingerprint, changed, color))?;
    }
    writeln!(out, "{} entries", entries.len())?;
    if cut_short {
        writeln!(out, "and the last one cut short, by a host killed while writing it")?;
    }
    Ok(())
}

/// How an entry's state type shows: its fingerprint, highlighted when it
/// differs from the previous entry's, in color on a terminal, else in words.
fn state_type(fingerprint: Option<u64>, changed: bool, color: bool) -> String {
    let fingerprint = fingerprint.map_or_else(|| String::from("unknown"), |fingerprint| format!("{fingerprint:016x}"));
    match (changed, color) {
        (false, _) => fingerprint,
        (true, true) => format!("{CHANGED}{fingerprint}{RESET}"),
        (true, false) => format!("{fingerprint} (changed)"),
    }
}

#[cfg(test)]
mod tests;
