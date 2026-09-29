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
