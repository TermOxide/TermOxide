use std::{
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

use color_eyre::Result;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher, event::ModifyKind};

use crate::build::{self, Package};

/// How long the sources must stay quiet before a rebuild starts: an editor's
/// save can touch a file several times.
const QUIET: Duration = Duration::from_millis(200);
/// How long a rebuild may run before it is killed: a real build can
/// legitimately take minutes.
const BUILD_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// Rebuilds the app's component whenever its sources change, on a thread of its
/// own so the host keeps running meanwhile.
pub(crate) struct Rebuilds {
    built: Receiver<PathBuf>,
    _watcher: RecommendedWatcher,
}

impl Rebuilds {
    /// Watches `package`'s `src` directory and manifest.
    pub(crate) fn start(package: Package) -> Result<Self> {
        let manifest = package.manifest();
        let sources = Path::new(package.manifest_dir).join("src");
        let (changes_tx, changes) = mpsc::channel();
        let mut watcher = notify::recommended_watcher({
            let (manifest, sources) = (manifest.clone(), sources.clone());
            move |event: notify::Result<Event>| {
                match event {
                    Ok(event) if is_edit(&event, &manifest, &sources) => {
                        let _ = changes_tx.send(());
                    },
                    Ok(_) => {},
                    Err(e) => log::warn!("watching the app's sources: {e}"),
                }
            }
        })?;
        watcher.watch(&sources, RecursiveMode::Recursive)?;
        // The directory, not the file: an editor that saves by replacing the
        // file would end a watch on the file itself.
        watcher.watch(Path::new(package.manifest_dir), RecursiveMode::NonRecursive)?;

        let (built_tx, built) = mpsc::channel();
        thread::spawn(move || rebuild_loop(&changes, QUIET, || build::rebuild(package, BUILD_TIMEOUT), &built_tx));
        Ok(Self { built, _watcher: watcher })
    }

    /// The newest component built since the last call, if any.
    pub(crate) fn latest(&self) -> Option<PathBuf> { self.built.try_iter().last() }
}

/// Whether `event` edits the app's sources or `manifest`.
///
/// Reads and metadata changes don't count: building reads every source, so
/// counting them would rebuild forever. Nor does anything else next to the
/// manifest, like `Cargo.lock` or `target`.
fn is_edit(event: &Event, manifest: &Path, sources: &Path) -> bool {
    let edits = matches!(
        event.kind,
        EventKind::Create(_)
            | EventKind::Remove(_)
            | EventKind::Modify(ModifyKind::Data(_) | ModifyKind::Name(_) | ModifyKind::Any)
    );
    edits && event.paths.iter().any(|path| path == manifest || path.starts_with(sources))
}

/// Builds once per batch of changes, until the watcher goes away, and sends
/// each component built to `built`.
///
/// A batch is whatever changes before the sources stay `quiet`, and whatever
/// changes during a build: those make one more build after it, never a second
/// one alongside it, which would fight over cargo's lock and target directory.
/// A failed build is logged and sends nothing, so the running generation stays.
fn rebuild_loop(
    changes: &Receiver<()>,
    quiet: Duration,
    mut build: impl FnMut() -> Result<PathBuf>,
    built: &Sender<PathBuf>,
) {
    while changes.recv().is_ok() {
        thread::sleep(quiet);
        while changes.try_recv().is_ok() {}

        log::info!("sources changed, rebuilding the guest");
        match build() {
            Ok(component) => {
                if built.send(component).is_err() {
                    return;
                }
            },
            Err(e) => log::error!("rebuilding the guest failed, keeping the running one: {e:#}"),
        }
    }
}

#[cfg(test)]
mod tests;
