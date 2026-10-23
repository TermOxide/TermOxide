#![allow(clippy::unwrap_used)]

use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU32, Ordering},
};

use super::*;

/// A fresh directory under the system's temporary directory, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "termoxide-snapshot-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

#[test]
fn a_saved_state_loads_back() {
    let dir = TempDir::new();
    let snapshot = Snapshot::in_dir(&dir.0.join("app"));

    snapshot.save(b"first").unwrap();
    snapshot.save(b"second").unwrap();

    assert_eq!(snapshot.load(false).unwrap(), Some(b"second".to_vec()));
    assert_eq!(
        fs::read_dir(dir.0.join("app")).unwrap().count(),
        1,
        "the partial write was left behind"
    );
}

#[test]
fn there_is_nothing_to_load_before_the_first_save() {
    let dir = TempDir::new();

    assert_eq!(Snapshot::in_dir(&dir.0).load(false).unwrap(), None);
}

#[test]
fn a_fresh_run_ignores_the_snapshot() {
    let dir = TempDir::new();
    let snapshot = Snapshot::in_dir(&dir.0);
    snapshot.save(b"saved").unwrap();

    assert_eq!(snapshot.load(true).unwrap(), None);
    assert!(snapshot.path().exists(), "only the run's own state replaces it");
}

#[test]
fn an_unreadable_snapshot_is_set_aside() {
    let dir = TempDir::new();
    let snapshot = Snapshot::in_dir(&dir.0);
    snapshot.save(b"garbled").unwrap();

    let aside = snapshot.set_aside().unwrap();

    assert_eq!(fs::read(aside).unwrap(), b"garbled");
    assert_eq!(snapshot.load(false).unwrap(), None);
}

#[test]
fn the_environment_overrides_the_choice_in_run_app() {
    assert!(persists(None, true));
    assert!(!persists(None, false));
    assert!(persists(Some("1"), false));
    assert!(!persists(Some("false"), true));
    assert!(persists(Some("anything else"), true));
}

#[test]
fn only_1_or_true_starts_fresh() {
    assert!(is_fresh(Some("1")));
    assert!(is_fresh(Some("true")));
    assert!(!is_fresh(Some("0")));
    assert!(!is_fresh(None));
}
