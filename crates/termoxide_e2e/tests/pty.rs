#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{COLS, ROWS, Session};

#[test]
fn startup_shows_the_first_frame_on_the_alternate_screen() {
    let session = Session::start();

    session.wait_for_startup();

    session.wait_until("a hidden cursor", |screen| screen.hide_cursor());
    let contents = session.contents();
    assert!(contents.contains("keys: 0"), "{contents}");
    assert!(contents.contains(&format!("viewport: {COLS}x{ROWS}")), "{contents}");
}

#[test]
fn keypress_updates_the_counter() {
    let mut session = Session::start();
    session.wait_for_startup();

    session.send(b"a");
    session.wait_for_text("keys: 1");

    session.send(b"bc");
    session.wait_for_text("keys: 3");
}

#[test]
fn resize_repaints_at_the_new_size() {
    let session = Session::start();
    session.wait_for_startup();

    session.resize(30, 100);

    session.wait_for_text("viewport: 100x30");
}

#[test]
fn raw_mode_is_on_while_running() {
    let session = Session::start();
    session.wait_for_startup();

    session.wait_for_raw_mode(true);
}

/// Send `keys` and wait for the probe to exit on its own, returning whether it
/// succeeded, once the terminal is handed back as it was found.
fn exit_with(session: &mut Session, keys: &[u8]) -> bool {
    session.wait_for_startup();
    session.wait_for_raw_mode(true);

    session.send(keys);

    let status = session.wait_for_exit();
    session.wait_for_restored_screen();
    session.wait_for_raw_mode(false);
    status.success()
}

#[test]
fn q_quits_and_restores_the_terminal() {
    assert!(exit_with(&mut Session::start(), b"q"));
}

#[test]
fn ctrl_c_quits_and_restores_the_terminal() {
    assert!(exit_with(&mut Session::start(), &[0x03]));
}

/// Where the probe's `explode` panics, as `src/bin/probe.rs:line:column`.
fn explode_location() -> String {
    let source = include_str!("../src/bin/probe.rs");
    let (index, line) = source
        .lines()
        .enumerate()
        .find(|(_, line)| line.contains("fn explode("))
        .unwrap();
    let column = line.find("panic!").unwrap() + 1;
    format!("src/bin/probe.rs:{}:{column}", index + 1)
}

/// The trimmed stack lines of a panic report, up to the `termoxide` boundary.
fn stack(contents: &str) -> Vec<&str> {
    let mut lines = contents
        .lines()
        .map(str::trim)
        .skip_while(|line| *line != "stack (most recent call first):");
    lines.next();
    let mut stack = Vec::new();
    for line in lines {
        stack.push(line);
        if line.starts_with("termoxide: App::") {
            return stack;
        }
    }
    panic!("no complete stack in:\n{contents}");
}

/// Panic the probe with `key` and check the report of a panic in `method`.
fn assert_panic_reported(key: &[u8], method: &str) {
    let mut session = Session::start();
    assert!(!exit_with(&mut session, key), "a panic must fail the probe");
    let boundary = format!("termoxide: App::{method}");
    session.wait_for_text(&boundary);

    let contents = session.contents();
    let message = format!("app panicked in `{method}`: probe panicked in {method} on purpose");
    assert!(contents.contains(&message), "{contents}");
    assert!(contents.contains(&explode_location()), "{contents}");

    let stack = stack(&contents);
    assert!(stack[0].starts_with("probe::explode "), "{stack:#?}");
    assert!(stack[0].ends_with(&explode_location()), "{stack:#?}");
    assert!(stack.len() >= 3, "`explode`, the method, the boundary: {stack:#?}");
    assert_eq!(stack.last(), Some(&boundary.as_str()));
    let runtime = ["std::", "core::", "alloc::", "tokio::"];
    assert!(
        !stack
            .iter()
            .any(|frame| runtime.iter().any(|prefix| frame.trim_start_matches('<').starts_with(prefix))),
        "{stack:#?}"
    );
}

#[test]
fn panic_in_handle_event_is_reported_on_the_restored_terminal() { assert_panic_reported(b"p", "handle_event"); }

#[test]
fn panic_with_backtraces_disabled_keeps_the_location() {
    let mut session = Session::start_with(&[("RUST_BACKTRACE", "0")]);
    assert!(!exit_with(&mut session, b"p"), "a panic must fail the probe");
    session.wait_for_text("no backtrace available (set RUST_BACKTRACE=1 to enable)");

    let contents = session.contents();
    assert!(contents.contains("app panicked in `handle_event`"), "{contents}");
    assert!(
        contents.contains(&format!("at crates/termoxide_e2e/{}", explode_location())),
        "{contents}"
    );
    assert!(!contents.contains("stack (most recent call first)"), "{contents}");
}
