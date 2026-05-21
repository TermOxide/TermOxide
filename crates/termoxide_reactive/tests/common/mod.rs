//! Shared test helpers for the `termoxide_reactive` integration tests.
//!
//! Effects in `reactive_graph` schedule their re-runs through
//! `any_spawner::Executor::spawn_local`, so every test that creates an
//! `Effect` needs a global executor installed and a tokio current-thread
//! runtime to drive it.

#![allow(dead_code)]

/// Install the tokio executor for `any_spawner` exactly once per process.
pub fn init_executor() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        any_spawner::Executor::init_tokio().expect("init tokio executor");
    });
}

/// Yield repeatedly so that any effects scheduled via the spawner get a
/// chance to run. A few yields are enough in practice — effects re-run
/// inside a microtask spawned on the local set.
pub async fn flush_effects() {
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
}
