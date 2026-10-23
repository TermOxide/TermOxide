//! Hot reload through a real terminal: the fixture app is built, run, and edited
//! while it runs.
//!
//! Not in tarpaulin's runs: traced with `--follow-exec`, the run aborts on a
//! segfault (most likely wasmtime's signal handling under ptrace), so CI runs
//! these with plain `cargo test` instead. See this crate's README.

#![cfg(all(unix, not(tarpaulin)))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant},
};

use common::{Session, build_deadline};
use portable_pty::CommandBuilder;

/// A copy of `fixtures/reload_app` under its own name, so tests running at once
/// don't overwrite each other's builds in the target directory they share.
struct Fixture {
    name: String,
    dir: PathBuf,
}

impl Fixture {
    fn copy(name: &str) -> Self {
        let name = format!("reload_app_{name}");
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(&name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();

        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/reload_app");
        let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let manifest = fs::read_to_string(fixture.join("Cargo.toml"))
            .unwrap()
            .replace("\"reload_app\"", &format!("\"{name}\""))
            .replace("\"../../../", &format!("\"{}/", crates.display()));
        fs::write(dir.join("Cargo.toml"), manifest).unwrap();
        for file in ["Cargo.lock", "src/lib.rs", "src/main.rs"] {
            fs::copy(fixture.join(file), dir.join(file)).unwrap();
        }
        Self { name, dir }
    }

    /// Where every fixture builds, so only the first build compiles the
    /// dependencies.
    fn target_dir() -> PathBuf { Path::new(env!("CARGO_TARGET_TMPDIR")).join("reload-target") }

    /// Builds the app's executable, the hot-reload host.
    fn build(&self) -> PathBuf {
        let status = Command::new(env!("CARGO"))
            .args(["build", "--quiet", "--manifest-path"])
            .arg(self.dir.join("Cargo.toml"))
            .env("CARGO_TARGET_DIR", Self::target_dir())
            .status()
            .unwrap();
        assert!(status.success(), "building {} failed", self.name);
        Self::target_dir().join("debug").join(&self.name)
    }

    /// Runs the app, with its logs kept in its own directory.
    fn run(&self) -> Session { self.run_with(&[]) }

    /// Runs the app with `env` set as well.
    fn run_with(&self, env: &[(&str, &str)]) -> Session {
        let mut command = CommandBuilder::new(self.build());
        for (key, value) in env {
            command.env(key, value);
        }
        command.cwd(&self.dir);
        command.env("CARGO", env!("CARGO"));
        command.env("CARGO_TARGET_DIR", Self::target_dir());
        command.env("XDG_STATE_HOME", self.dir.join("state"));
        Session::spawn(command)
    }

    /// Where a persistent run keeps the app's state.
    fn snapshot(&self) -> PathBuf { self.dir.join("state/termoxide").join(&self.name).join("state.bin") }

    /// Makes each `(from, to)` replacement in the app's `lib.rs`, then saves it
    /// once, as an editor would.
    fn edit(&self, replacements: &[(&str, &str)]) {
        let path = self.dir.join("src/lib.rs");
        let mut source = fs::read_to_string(&path).unwrap();
        for (from, to) in replacements {
            assert!(source.contains(from), "{from:?} is not in the fixture");
            source = source.replacen(from, to, 1);
        }
        fs::write(&path, source).unwrap();
    }

    /// Waits for the run's log to contain `text`, and returns it.
    fn wait_for_log(&self, text: &str) -> String {
        let logs = self.dir.join("state/termoxide").join(&self.name).join("logs");
        let deadline = Instant::now() + build_deadline();
        loop {
            let log: String = fs::read_dir(&logs)
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(|entry| fs::read_to_string(entry.path()).ok())
                .collect();
            if log.contains(text) {
                return log;
            }
            assert!(Instant::now() < deadline, "the log never said {text:?}:\n{log}");
            thread::sleep(Duration::from_millis(100));
        }
    }
}

/// Runs `fixture` and presses `a` `presses` times.
fn run_and_press(fixture: &Fixture, presses: usize) -> Session {
    let mut session = fixture.run();
    session.wait_for_text_within(build_deadline(), "presses: 0");
    session.send(&b"a".repeat(presses));
    session.wait_for_text(&format!("presses: {presses}"));
    session
}

#[test]
fn an_edit_shows_while_the_app_runs_and_keeps_its_state() {
    let fixture = Fixture::copy("edit");
    let mut session = run_and_press(&fixture, 3);

    fixture.edit(&[("\"presses: {}\"", "\"edited: {}\"")]);

    session.wait_for_text_within(build_deadline(), "edited: 3");
    session.send(b"a");
    session.wait_for_text("edited: 4");
}

#[test]
fn a_broken_edit_keeps_the_app_running_until_it_is_fixed() {
    let fixture = Fixture::copy("broken");
    let mut session = run_and_press(&fixture, 1);

    fixture.edit(&[("\"presses: {}\"", "\"presses: {}\" +")]);
    fixture.wait_for_log("rebuilding the guest failed");
    session.send(b"a");
    session.wait_for_text("presses: 2");

    fixture.edit(&[("\"presses: {}\" +", "\"fixed: {}\"")]);
    session.wait_for_text_within(build_deadline(), "fixed: 2");
}

#[test]
fn a_new_state_field_starts_the_app_afresh() {
    let fixture = Fixture::copy("state");
    let session = run_and_press(&fixture, 2);

    fixture.edit(&[
        (
            "    count: Signal<u32>,
}",
            "    count: Signal<u32>,
    extra: Signal<u8>,
}",
        ),
        (
            "Self { count: Signal::new(0) }",
            "Self { count: Signal::new(0), extra: Signal::new(0) }",
        ),
        ("\"presses: {}\"", "\"afresh: {}\""),
    ]);

    session.wait_for_text_within(build_deadline(), "afresh: 0");
    fixture.wait_for_log("starts afresh");
}

#[test]
fn a_panic_restarts_the_app_with_its_count() {
    let fixture = Fixture::copy("panic");
    let mut session = run_and_press(&fixture, 2);

    session.send(b"p");
    fixture.wait_for_log("the app crashed");
    session.send(b"a");

    session.wait_for_text("presses: 3");
}

#[test]
fn a_runaway_call_is_stopped_and_the_app_restarted_with_its_count() {
    let fixture = Fixture::copy("loop");
    let mut session = run_and_press(&fixture, 1);

    session.send(b"l");
    fixture.wait_for_log("the app crashed");
    session.send(b"a");

    session.wait_for_text("presses: 2");
}

#[test]
fn crashing_too_often_stops_the_app_until_the_next_rebuild() {
    let fixture = Fixture::copy("stop");
    let mut session = run_and_press(&fixture, 2);

    session.send(b"pppp");
    session.wait_for_text("stopped until the next rebuild");
    session.send(b"a");
    fixture.edit(&[("\"presses: {}\"", "\"fixed: {}\"")]);

    session.wait_for_text_within(build_deadline(), "fixed: 2");
    session.send(b"a");
    session.wait_for_text("fixed: 3");
}

#[test]
fn ctrl_c_quits_a_stopped_app() {
    let fixture = Fixture::copy("quit");
    let mut session = run_and_press(&fixture, 1);
    session.send(b"pppp");
    session.wait_for_text("stopped until the next rebuild");

    session.send(&[0x03]);

    assert!(session.wait_for_exit().success());
}

#[test]
fn a_lost_subprocess_is_restarted_from_the_hosts_state() {
    let fixture = Fixture::copy("lost");
    let mut session = run_and_press(&fixture, 2);
    let log = fixture.wait_for_log("subprocess ");
    let pid = log
        .lines()
        .find_map(|line| line.split("subprocess ").nth(1)?.strip_suffix(" connected"))
        .expect("the subprocess's pid in the log");

    assert!(Command::new("kill").args(["-9", pid]).status().unwrap().success());
    session.send(b"a");
    fixture.wait_for_log("restarted the subprocess");
    session.send(b"a");

    session.wait_for_text("presses: 3");
}

#[test]
fn a_crash_right_after_a_reload_goes_back_to_the_previous_generation() {
    let fixture = Fixture::copy("back");
    let mut session = run_and_press(&fixture, 1);
    fixture.edit(&[
        ("self.count.update(|count| *count += 1);", "panic!(\"broken by the edit\");"),
        ("\"presses: {}\"", "\"broken: {}\""),
    ]);
    session.wait_for_text_within(build_deadline(), "broken: 1");

    session.send(b"a");

    session.wait_for_text("presses: 1");
    fixture.wait_for_log("going back to the generation before the last reload");
    session.send(b"a");
    session.wait_for_text("presses: 2");
}

const PERSIST: (&str, &str) = ("TERMOXIDE_HOT_RELOAD_PERSIST", "1");

/// Quits the app with `q` and waits for it to exit.
fn quit(mut session: Session) {
    session.send(b"q");
    assert!(session.wait_for_exit().success());
}

#[test]
fn the_state_survives_a_restart_of_the_host() {
    let fixture = Fixture::copy("persist");
    let mut session = fixture.run_with(&[PERSIST]);
    session.wait_for_text_within(build_deadline(), "presses: 0");
    session.send(b"aaa");
    session.wait_for_text("presses: 3");
    quit(session);

    let session = fixture.run_with(&[PERSIST]);
    session.wait_for_text_within(build_deadline(), "presses: 3");
    quit(session);

    let session = fixture.run_with(&[PERSIST, ("TERMOXIDE_HOT_RELOAD_FRESH", "1")]);
    session.wait_for_text_within(build_deadline(), "presses: 0");
}

#[test]
fn a_damaged_snapshot_is_set_aside_and_the_app_starts_afresh() {
    let fixture = Fixture::copy("damaged");
    let mut session = fixture.run_with(&[PERSIST]);
    session.wait_for_text_within(build_deadline(), "presses: 0");
    session.send(b"aa");
    session.wait_for_text("presses: 2");
    quit(session);
    // One entry holding the fingerprint, then a varint that never ends: the
    // state type matches, the bytes don't decode.
    let trace = fs::read(fixture.snapshot()).unwrap();
    let fingerprint = &first_state(&trace)[..8];
    fs::write(fixture.snapshot(), [&[0, 9], fingerprint, &[0xFF]].concat()).unwrap();

    let session = fixture.run_with(&[PERSIST]);

    session.wait_for_text_within(build_deadline(), "presses: 0");
    assert!(fixture.snapshot().with_extension("bin.unreadable").exists());
}

/// The state in a trace's first entry: a postcard `{ at: u64, state: &[u8] }`,
/// two varints and the bytes.
fn first_state(trace: &[u8]) -> &[u8] {
    let varint_end = |from: usize| from + trace[from..].iter().position(|byte| byte & 0x80 == 0).unwrap() + 1;
    let len_at = varint_end(0);
    let state_at = varint_end(len_at);
    &trace[state_at..state_at + usize::from(trace[len_at])]
}

#[test]
fn crashing_too_often_sets_the_trace_aside_and_the_next_run_starts_afresh() {
    let fixture = Fixture::copy("crashed");
    let mut session = fixture.run_with(&[PERSIST]);
    session.wait_for_text_within(build_deadline(), "presses: 0");
    session.send(b"aa");
    session.wait_for_text("presses: 2");

    session.send(b"pppp");
    session.wait_for_text("stopped until the next rebuild");
    fixture.wait_for_log("the state trace is kept");
    session.send(&[0x03]);
    assert!(session.wait_for_exit().success());

    assert!(fixture.snapshot().with_extension("bin.crashed").exists());
    let session = fixture.run_with(&[PERSIST]);
    session.wait_for_text_within(build_deadline(), "presses: 0");
}
