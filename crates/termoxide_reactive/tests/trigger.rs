//! Behavior tests for `Trigger`.

mod common;

use std::cell::RefCell;
use std::rc::Rc;
use termoxide_reactive::runtime::Owner;
use termoxide_reactive::{Effect, Trigger};

#[tokio::test(flavor = "current_thread")]
async fn notify_wakes_tracking_effect() {
    common::init_executor();
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let owner = Owner::new();
            owner.set();
            let t = Trigger::new();
            let runs = Rc::new(RefCell::new(0u32));
            let runs_c = runs.clone();
            Effect::new(move |_: Option<()>| {
                t.track();
                *runs_c.borrow_mut() += 1;
            });
            common::flush_effects().await;
            assert_eq!(*runs.borrow(), 1);
            t.notify();
            common::flush_effects().await;
            assert_eq!(*runs.borrow(), 2);
            drop(owner);
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn multiple_notifies_cause_multiple_runs() {
    common::init_executor();
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let owner = Owner::new();
            owner.set();
            let t = Trigger::new();
            let runs = Rc::new(RefCell::new(0u32));
            let runs_c = runs.clone();
            Effect::new(move |_: Option<()>| {
                t.track();
                *runs_c.borrow_mut() += 1;
            });
            common::flush_effects().await;
            t.notify();
            common::flush_effects().await;
            t.notify();
            common::flush_effects().await;
            t.notify();
            common::flush_effects().await;
            assert_eq!(*runs.borrow(), 4); // 1 initial + 3 notifies
            drop(owner);
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn notify_without_track_does_not_run_effect() {
    common::init_executor();
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let owner = Owner::new();
            owner.set();
            let t = Trigger::new();
            let runs = Rc::new(RefCell::new(0u32));
            let runs_c = runs.clone();
            Effect::new(move |_: Option<()>| {
                // No t.track() call.
                *runs_c.borrow_mut() += 1;
            });
            common::flush_effects().await;
            let initial = *runs.borrow();
            t.notify();
            common::flush_effects().await;
            assert_eq!(*runs.borrow(), initial);
            drop(owner);
        })
        .await;
}

#[test]
fn trigger_is_copy() {
    let t = Trigger::new();
    let t2 = t;
    // Both handles refer to the same underlying trigger; just call methods.
    t.notify();
    t2.notify();
}

#[test]
fn debug_prints_trigger_marker() {
    let t = Trigger::new();
    let s = format!("{:?}", t);
    assert!(s.contains("Trigger"));
}

#[test]
fn default_constructs_a_trigger() {
    let _ = Trigger::default();
}
