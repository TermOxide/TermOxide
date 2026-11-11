#![allow(clippy::unwrap_used)]

use std::{
    process::Command,
    time::{Duration, Instant},
};

use super::*;

#[cfg(unix)]
#[test]
fn a_build_past_its_timeout_is_stopped() {
    let started = Instant::now();
    let mut child = Command::new("sleep").arg("600").spawn().unwrap();

    let result = wait_with_timeout(&mut child, Duration::from_millis(200));

    assert!(result.is_err());
    // Far short of the child's 10 minutes, with room for a slow reap: under
    // tarpaulin's tracing, killing and reaping it takes seconds.
    assert!(started.elapsed() < Duration::from_secs(60));
    assert!(child.try_wait().unwrap().is_some(), "the child was not reaped");
}

#[cfg(unix)]
#[test]
fn a_build_that_finishes_in_time_reports_its_status() {
    let mut child = Command::new("false").spawn().unwrap();

    let status = wait_with_timeout(&mut child, Duration::from_secs(5)).unwrap();

    assert!(!status.success());
}

const APP: Package = Package { manifest_dir: "/apps/clock", app: None };

/// A build's exit status with `code`.
#[cfg(unix)]
fn exited(code: u8) -> ExitStatus { std::os::unix::process::ExitStatusExt::from_raw(i32::from(code) << 8) }

/// A build's exit status with `code`.
#[cfg(windows)]
fn exited(code: u8) -> ExitStatus { std::os::windows::process::ExitStatusExt::from_raw(u32::from(code)) }

/// Cargo's JSON line for an artifact of `kinds` made of `files`.
fn artifact(kinds: &str, files: &str) -> String {
    format!(r#"{{"reason":"compiler-artifact","target":{{"kind":[{kinds}]}},"filenames":[{files}]}}"#)
}

#[test]
fn the_component_is_the_wasm_file_of_the_cdylib() {
    let stdout = [
        artifact(r#""lib""#, r#""/t/libserde.rlib""#),
        artifact(r#""cdylib","rlib""#, r#""/t/libclock.rlib","/t/clock.wasm""#),
        String::from(r#"{"reason":"build-finished","success":true}"#),
    ]
    .join("\n");

    assert_eq!(
        component(APP, exited(0), stdout.as_bytes()).unwrap(),
        Path::new("/t/clock.wasm")
    );
}

#[test]
fn a_lib_that_is_no_cdylib_is_said_to_need_one() {
    let stdout = artifact(r#""lib""#, r#""/t/libclock.rlib""#);

    let error = component(APP, exited(0), stdout.as_bytes()).unwrap_err();

    assert!(error.to_string().contains("cdylib"), "{error}");
}

#[test]
fn a_failed_build_has_no_component_even_with_one_in_its_output() {
    let stdout = artifact(r#""cdylib""#, r#""/t/clock.wasm""#);

    assert!(component(APP, exited(101), stdout.as_bytes()).is_err());
}
