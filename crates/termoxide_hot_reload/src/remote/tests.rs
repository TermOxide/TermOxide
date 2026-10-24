use std::time::{Duration, Instant};

use super::*;

#[test]
fn up_to_the_limit_restarts_are_allowed_within_the_window() {
    let mut restarts = Restarts::default();
    let start = Instant::now();

    for i in 0..MAX_RESTARTS {
        assert!(restarts.allow(start + Duration::from_secs(i as u64)), "restart {i}");
    }
    assert!(!restarts.allow(start + Duration::from_secs(MAX_RESTARTS as u64)));
}

#[test]
fn restarts_older_than_the_window_no_longer_count() {
    let mut restarts = Restarts::default();
    let start = Instant::now();
    for _ in 0..MAX_RESTARTS {
        assert!(restarts.allow(start));
    }

    assert!(restarts.allow(start + RESTART_WINDOW));
}

#[test]
fn a_reset_forgets_every_restart() {
    let mut restarts = Restarts::default();
    let start = Instant::now();
    for _ in 0..MAX_RESTARTS {
        assert!(restarts.allow(start));
    }

    restarts.reset();

    assert!(restarts.allow(start));
}
