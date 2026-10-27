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
