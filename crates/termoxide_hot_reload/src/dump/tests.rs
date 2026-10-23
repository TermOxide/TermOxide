#![allow(clippy::unwrap_used)]

use super::*;

fn parse(args: &[&str]) -> Result<Args> { Args::parse(args.iter().map(OsString::from)) }

#[test]
fn the_trace_is_enough() {
    assert_eq!(parse(&["state.bin"]).unwrap(), Args {
        trace: PathBuf::from("state.bin"),
        manifest_dir: None
    });
}

#[test]
fn the_manifest_can_be_given_as_the_file_or_its_directory() {
    let expected = Args {
        trace: PathBuf::from("state.bin"),
        manifest_dir: Some(PathBuf::from("apps/clock")),
    };

    assert_eq!(parse(&["state.bin", "--manifest", "apps/clock/Cargo.toml"]).unwrap(), expected);
    assert_eq!(parse(&["--manifest", "apps/clock", "state.bin"]).unwrap(), expected);
}

#[test]
fn a_missing_trace_or_a_second_one_is_refused() {
    assert!(parse(&[]).is_err());
    assert!(parse(&["a.bin", "b.bin"]).is_err());
    assert!(parse(&["a.bin", "--manifest"]).is_err());
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
