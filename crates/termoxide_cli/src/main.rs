use std::path::PathBuf;

use any_spawner::Executor;
use clap::{Args, Parser, Subcommand};
use color_eyre::Result;
use termoxide_hot_reload::cli::{self, Aside, Dump};

/// Run TermOxide apps from their crate, hot reloaded while you edit them.
#[derive(Parser)]
#[command(name = "termoxide", version)]
struct Termoxide {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Run(Run),
    Dump(DumpArgs),
}

/// Run the app of a crate, rebuilt and swapped in as its source changes.
///
/// The crate's lib must list `cdylib` and `rlib` in its `crate-type`, have a
/// `guest` feature enabling `termoxide_hot_reload/guest`, and declare its app
/// with `wasm_app!`. The crate needs no `main.rs`.
///
/// Whether the app keeps its state across runs is, in order: `--persist` or
/// `--no-persist`, then `TERMOXIDE_PERSIST=1` or `0`, then `persist = true` or
/// `false` in the app's `config.toml`, next to its logs, then `persistent`
/// under `[package.metadata.termoxide]`.
#[derive(Args)]
struct Run {
    /// The app's crate folder.
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Keep the app's state across runs.
    #[arg(long, conflicts_with = "no_persist")]
    persist: bool,

    /// Don't keep the app's state across runs.
    #[arg(long)]
    no_persist: bool,

    /// Start from a clean state rather than the last run's, as
    /// `TERMOXIDE_FRESH=1` does.
    #[arg(long)]
    fresh: bool,
}

/// Print the state trace a persistent run left, as the app reads it.
///
/// Each entry shows when it was recorded, its state type's fingerprint,
/// highlighted where it changed, and the app as JSON. Only the app's own
/// component can decode its state, so this builds it from the app's crate.
#[derive(Args)]
struct DumpArgs {
    /// A trace, or the crate folder of the app that wrote it.
    #[arg(default_value = ".")]
    path: PathBuf,

    /// The binary that wrote the trace, when the crate has several.
    #[arg(long, value_name = "NAME")]
    bin: Option<String>,

    /// The trace set aside when the app kept crashing.
    #[arg(long, conflicts_with = "unreadable")]
    crashed: bool,

    /// The trace set aside when its state no longer decoded.
    #[arg(long)]
    unreadable: bool,

    /// The app's crate, its folder or its `Cargo.toml`, when it moved since
    /// the trace was written.
    #[arg(long, value_name = "PATH")]
    manifest: Option<PathBuf>,
}

impl Run {
    fn persist(&self) -> Option<bool> {
        match (self.persist, self.no_persist) {
            (true, _) => Some(true),
            (_, true) => Some(false),
            _ => None,
        }
    }
}

impl From<DumpArgs> for Dump {
    fn from(args: DumpArgs) -> Self {
        let aside = match (args.crashed, args.unreadable) {
            (true, _) => Some(Aside::Crashed),
            (_, true) => Some(Aside::Unreadable),
            _ => None,
        };
        Self { path: args.path, bin: args.bin, aside, manifest: args.manifest }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    color_eyre::install()?;
    Executor::init_tokio()?;

    if cli::is_child() {
        return cli::serve();
    }
    match Termoxide::parse().command {
        Command::Run(run) => {
            let persist = run.persist();
            tokio::task::LocalSet::new()
                .run_until(cli::run(&run.path, persist, run.fresh))
                .await
        },
        Command::Dump(args) => cli::dump(&args.into()),
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn the_arguments_are_well_defined() { Termoxide::command().debug_assert(); }
}
