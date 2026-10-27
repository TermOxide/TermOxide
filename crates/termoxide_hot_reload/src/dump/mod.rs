//! `termoxide_dump`: prints a state trace a persistent run left, each entry
//! with when it was recorded, as the app that wrote it reads it. Only the app's
//! own component can decode its state, so this builds it from the crate the
//! trace's header names.

use std::{
    ffi::OsString,
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
    schema,
    snapshot::{self, Entry},
};

const USAGE: &str = "usage: termoxide_dump <trace> [--manifest <path to the app's Cargo.toml>]";

/// ANSI bold yellow, for a state type that changed since the previous entry.
const CHANGED: &str = "\x1b[1;33m";
/// ANSI back to the terminal's own style.
const RESET: &str = "\x1b[0m";

/// What to dump.
#[derive(Debug, PartialEq, Eq)]
struct Args {
    trace: PathBuf,
    /// The app's crate, when it isn't where the trace says: it moved.
    manifest_dir: Option<PathBuf>,
}

impl Args {
    /// Reads the arguments after the program's name.
    fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self> {
        let mut args = args.into_iter();
        let (mut trace, mut manifest_dir) = (None, None);
        while let Some(arg) = args.next() {
            if arg == "--manifest" {
                let manifest = PathBuf::from(args.next().ok_or_else(|| eyre!("--manifest needs a path\n{USAGE}"))?);
                // Either the manifest or the directory holding it.
                manifest_dir = Some(match manifest.file_name() {
                    Some(name) if name == "Cargo.toml" => manifest.parent().map(Path::to_owned).unwrap_or_default(),
                    _ => manifest,
                });
            } else if trace.is_none() {
                trace = Some(PathBuf::from(arg));
            } else {
                return Err(eyre!("unexpected argument {}\n{USAGE}", arg.to_string_lossy()));
            }
        }
        Ok(Self { trace: trace.ok_or_else(|| eyre!(USAGE))?, manifest_dir })
    }
}

/// What the `termoxide_dump` binary runs.
pub fn main() -> Result<()> {
    let args = Args::parse(std::env::args_os().skip(1))?;
    let bytes = fs::read(&args.trace)?;
    let trace = snapshot::parse(&bytes).ok_or_else(|| eyre!("{} isn't a state trace", args.trace.display()))?;

    let manifest_dir = match args.manifest_dir {
        Some(dir) => dir.to_string_lossy().into_owned(),
        None => trace.manifest_dir.to_owned(),
    };
    // A `Package` holds what `env!` gives `run_app!`: leaked once, for the
    // one build this process makes. The guest follows this tool's profile, as
    // it follows the app's in a run.
    let guest = build::guest(Package {
        manifest_dir: Box::leak(manifest_dir.into_boxed_str()),
        release: !cfg!(debug_assertions),
    })?;

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
