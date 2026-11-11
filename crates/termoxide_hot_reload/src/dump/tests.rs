#![allow(clippy::unwrap_used)]

use super::*;

fn trace_at(path: &Path) -> Dump { Dump { path: path.to_owned(), bin: None, aside: None, manifest: None } }

#[test]
fn the_manifest_can_be_given_as_the_file_or_its_directory() {
    assert_eq!(manifest_dir(Path::new("apps/clock/Cargo.toml")), Path::new("apps/clock"));
    assert_eq!(manifest_dir(Path::new("apps/clock")), Path::new("apps/clock"));
}

#[test]
fn a_trace_file_has_no_binary_or_set_aside_trace_to_pick() {
    let file = std::env::current_exe().unwrap();
    let dump = Dump { aside: Some(Aside::Unreadable), ..trace_at(&file) };

    assert!(dump.trace().is_err());
    assert_eq!(trace_at(&file).trace().unwrap(), (file, None));
}

#[test]
fn a_state_type_shows_as_its_fingerprint() {
    assert_eq!(state_type(Some(0x2A), false, true), "000000000000002a");
    assert_eq!(state_type(None, false, false), "unknown");
}

#[test]
fn a_changed_state_type_is_colored_on_a_terminal_and_said_elsewhere() {
    assert_eq!(state_type(Some(0x2A), true, true), "\x1b[1;33m000000000000002a\x1b[0m");
    assert_eq!(state_type(Some(0x2A), true, false), "000000000000002a (changed)");
}
