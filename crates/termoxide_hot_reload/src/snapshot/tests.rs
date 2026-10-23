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

/// The states traced at `path`, in order.
fn traced(path: &Path) -> Vec<Vec<u8>> {
    entries(&fs::read(path).unwrap())
        .0
        .iter()
        .map(|entry| entry.state.to_vec())
        .collect()
}

#[test]
fn a_run_traces_every_state_and_the_next_one_starts_from_the_last() {
    let dir = TempDir::new();
    let run = Snapshot::in_dir(&dir.0.join("app"));

    run.record(b"first").unwrap();
    run.record(b"second").unwrap();

    assert_eq!(traced(run.path()), [b"first".to_vec(), b"second".to_vec()]);
    assert_eq!(
        Snapshot::in_dir(&dir.0.join("app")).load(false).unwrap(),
        Some(b"second".to_vec())
    );
    assert_eq!(
        fs::read_dir(dir.0.join("app")).unwrap().count(),
        1,
        "the partial write was left behind"
    );
}

#[test]
fn a_new_run_replaces_the_last_runs_trace() {
    let dir = TempDir::new();
    Snapshot::in_dir(&dir.0).record(b"last run").unwrap();

    let run = Snapshot::in_dir(&dir.0);
    run.record(b"this run").unwrap();

    assert_eq!(traced(run.path()), [b"this run".to_vec()]);
}

#[test]
fn an_entry_cut_short_is_skipped() {
    let dir = TempDir::new();
    let run = Snapshot::in_dir(&dir.0);
    run.record(b"whole").unwrap();
    run.record(b"cut short").unwrap();
    let trace = fs::read(run.path()).unwrap();
    let cut = &trace[..trace.len() - 2];
    fs::write(run.path(), cut).unwrap();

    assert_eq!(run.load(false).unwrap(), Some(b"whole".to_vec()));
    assert!(entries(cut).1);
}

#[test]
fn a_trace_without_a_complete_entry_is_invalid() {
    let dir = TempDir::new();
    let run = Snapshot::in_dir(&dir.0);
    fs::write(run.path(), [0xFF]).unwrap();

    assert_eq!(run.load(false).unwrap_err().kind(), io::ErrorKind::InvalidData);
}

#[test]
fn there_is_nothing_to_load_before_the_first_run() {
    let dir = TempDir::new();

    assert_eq!(Snapshot::in_dir(&dir.0).load(false).unwrap(), None);
}

#[test]
fn a_fresh_run_ignores_the_trace() {
    let dir = TempDir::new();
    let run = Snapshot::in_dir(&dir.0);
    run.record(b"saved").unwrap();

    assert_eq!(run.load(true).unwrap(), None);
    assert!(run.path().exists(), "only the run's own state replaces it");
}

#[test]
fn a_trace_set_aside_keeps_its_entries_and_the_next_state_starts_another() {
    let dir = TempDir::new();
    let run = Snapshot::in_dir(&dir.0);
    run.record(b"crashing").unwrap();

    let aside = run.set_aside(Aside::Crashed).unwrap();
    run.record(b"after").unwrap();

    assert_eq!(aside, dir.0.join("state.bin.crashed"));
    assert_eq!(traced(&aside), [b"crashing".to_vec()]);
    assert_eq!(traced(run.path()), [b"after".to_vec()]);
}

#[test]
fn an_unreadable_trace_is_set_aside_under_its_own_name() {
    let dir = TempDir::new();
    let run = Snapshot::in_dir(&dir.0);
    run.record(b"garbled").unwrap();

    let aside = run.set_aside(Aside::Unreadable).unwrap();

    assert_eq!(aside, dir.0.join("state.bin.unreadable"));
    assert_eq!(run.load(false).unwrap(), None);
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
