#![allow(clippy::unwrap_used)]

use std::{cell::Cell, path::PathBuf, sync::mpsc, time::Duration};

use color_eyre::eyre::eyre;
use notify::event::{AccessKind, CreateKind, DataChange, MetadataKind, RenameMode};

use super::*;

#[test]
fn changes_saved_before_a_build_starts_make_one_build() {
    let (changes_tx, changes) = mpsc::channel();
    let (built_tx, built) = mpsc::channel();
    for _ in 0..3 {
        changes_tx.send(()).unwrap();
    }
    drop(changes_tx);
    let builds = Cell::new(0);

    rebuild_loop(
        &changes,
        Duration::ZERO,
        || {
            builds.set(builds.get() + 1);
            Ok(PathBuf::from("app.wasm"))
        },
        &built_tx,
    );

    assert_eq!(builds.get(), 1);
    assert_eq!(built.try_iter().collect::<Vec<_>>(), [PathBuf::from("app.wasm")]);
}

#[test]
fn changes_saved_during_a_build_make_one_more_build_after_it() {
    let (changes_tx, changes) = mpsc::channel();
    let (built_tx, built) = mpsc::channel();
    changes_tx.send(()).unwrap();
    // The build saves twice while it runs, then lets the watcher go away.
    let mut during = Some(changes_tx);
    let builds = Cell::new(0);

    rebuild_loop(
        &changes,
        Duration::ZERO,
        || {
            builds.set(builds.get() + 1);
            if let Some(changes_tx) = during.take() {
                changes_tx.send(()).unwrap();
                changes_tx.send(()).unwrap();
            }
            Ok(PathBuf::from(format!("app-{}.wasm", builds.get())))
        },
        &built_tx,
    );

    assert_eq!(builds.get(), 2);
    assert_eq!(built.try_iter().last(), Some(PathBuf::from("app-2.wasm")));
}

#[test]
fn a_failed_build_sends_nothing() {
    let (changes_tx, changes) = mpsc::channel();
    let (built_tx, built) = mpsc::channel();
    changes_tx.send(()).unwrap();
    drop(changes_tx);

    rebuild_loop(&changes, Duration::ZERO, || Err(eyre!("does not compile")), &built_tx);

    assert_eq!(built.try_iter().count(), 0);
}

#[test]
fn only_edits_to_the_sources_or_the_manifest_count() {
    let manifest = Path::new("/app/Cargo.toml");
    let sources = Path::new("/app/src");
    let event = |kind, path: &str| Event::new(kind).add_path(PathBuf::from(path));
    let written = EventKind::Modify(ModifyKind::Data(DataChange::Content));

    assert!(is_edit(&event(written, "/app/src/lib.rs"), manifest, sources));
    assert!(is_edit(&event(written, "/app/Cargo.toml"), manifest, sources));
    assert!(is_edit(
        &event(EventKind::Create(CreateKind::File), "/app/src/view.rs"),
        manifest,
        sources
    ));
    assert!(is_edit(
        &event(EventKind::Modify(ModifyKind::Name(RenameMode::To)), "/app/src/lib.rs"),
        manifest,
        sources
    ));

    assert!(!is_edit(
        &event(EventKind::Access(AccessKind::Any), "/app/src/lib.rs"),
        manifest,
        sources
    ));
    assert!(!is_edit(
        &event(
            EventKind::Modify(ModifyKind::Metadata(MetadataKind::AccessTime)),
            "/app/src/lib.rs"
        ),
        manifest,
        sources
    ));
    assert!(!is_edit(&event(written, "/app/Cargo.lock"), manifest, sources));
    assert!(!is_edit(&event(written, "/app/target/debug/app"), manifest, sources));
}
