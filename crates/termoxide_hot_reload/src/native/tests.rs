#![allow(clippy::unwrap_used)]

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU32, Ordering},
};

use ratatui::style::Style;
use serde::{Deserialize, Serialize};
use termoxide_event::event::{KeyCode, KeyEvent, KeyModifiers};
use termoxide_reactive::{Owner, Signal, Unrecorded};

use super::*;
use crate::snapshot;

/// Counts ticks, and `h` presses it doesn't record.
#[derive(Clone, Copy, Serialize, Deserialize)]
struct Counter {
    ticks: Signal<u32>,
    hidden: Signal<u32, Unrecorded>,
}

impl Counter {
    fn new() -> Self { Self { ticks: Signal::new(0), hidden: Signal::new(0).unrecorded() } }
}

impl App for Counter {
    fn track_view(&self) { let _ = self.ticks.get(); }

    fn on_tick(&self) { self.ticks.update(|n| *n += 1); }

    fn handle_event(&self, _: Event) -> bool {
        self.hidden.update(|n| *n += 1);
        false
    }

    fn build_view(&self, viewport: Rect) -> ViewNode {
        ViewNode::text(viewport, self.ticks.get_untracked().to_string(), Style::default())
    }
}

/// A fresh directory under the system's temporary directory, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "termoxide-native-test-{}-{}",
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

fn start(dir: &Path) -> Persisting<Counter> {
    Persisting::start(Counter::new, Snapshot::in_dir(dir, "/apps/counter"), false).unwrap()
}

fn entries(dir: &Path) -> usize {
    let trace = fs::read(dir.join(snapshot::FILE)).unwrap();
    snapshot::parse(&trace).unwrap().entries.len()
}

#[test]
fn a_run_traces_its_start_then_each_change_of_a_recorded_signal() {
    let owner = Owner::new();
    owner.set();
    let dir = TempDir::new();
    let app = start(&dir.0);

    app.on_tick();
    app.handle_event(Event::KeyPress(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE)));
    app.on_tick();

    assert_eq!(entries(&dir.0), 3);
}

#[test]
fn the_next_run_starts_from_the_last_state_traced() {
    let owner = Owner::new();
    owner.set();
    let dir = TempDir::new();
    let app = start(&dir.0);
    app.on_tick();
    app.on_tick();

    assert_eq!(start(&dir.0).app.ticks.get_untracked(), 2);
}

#[test]
fn a_trace_that_isnt_one_is_set_aside_and_the_app_starts_afresh() {
    let owner = Owner::new();
    owner.set();
    let dir = TempDir::new();
    fs::write(dir.0.join(snapshot::FILE), "not a trace").unwrap();

    assert_eq!(start(&dir.0).app.ticks.get_untracked(), 0);
    assert!(dir.0.join("state.bin.unreadable").exists());
}
