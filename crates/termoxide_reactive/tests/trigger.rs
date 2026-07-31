//! Behavior tests for `Trigger`.

mod common;

use std::{cell::RefCell, rc::Rc};

use termoxide_reactive::{Effect, Trigger};

#[tokio::test(flavor = "current_thread")]
async fn notify_wakes_tracking_effect() {
    common::run_reactive(async {
        let t = Trigger::new();
        let runs = Rc::new(RefCell::new(0u32));
        Effect::new({
            let runs = runs.clone();
            move |_: Option<()>| {
                t.track();
                *runs.borrow_mut() += 1;
            }
        });
        common::flush_effects().await;
        assert_eq!(*runs.borrow(), 1);
        t.notify();
        common::flush_effects().await;
        assert_eq!(*runs.borrow(), 2);
    })
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn multiple_notifies_cause_multiple_runs() {
    common::run_reactive(async {
        let t = Trigger::new();
        let runs = Rc::new(RefCell::new(0u32));
        Effect::new({
            let runs = runs.clone();
            move |_: Option<()>| {
                t.track();
                *runs.borrow_mut() += 1;
            }
        });
        common::flush_effects().await;
        t.notify();
        common::flush_effects().await;
        t.notify();
        common::flush_effects().await;
        t.notify();
        common::flush_effects().await;
        assert_eq!(*runs.borrow(), 4); // 1 initial + 3 notifies
    })
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn notifies_coalesce_until_flushed() {
    // Once an effect is scheduled, further notifies do
    // not re-queue it.
    common::run_reactive(async {
        let t = Trigger::new();
        let runs = Rc::new(RefCell::new(0u32));
        Effect::new({
            let runs = runs.clone();
            move |_: Option<()>| {
                t.track();
                *runs.borrow_mut() += 1;
            }
        });
        common::flush_effects().await;
        assert_eq!(*runs.borrow(), 1, "initial run");

        t.notify();
        t.notify();
        t.notify();
        common::flush_effects().await;
        assert_eq!(*runs.borrow(), 2, "three notifies between flushes collapse into one re-run");
    })
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn notify_without_track_does_not_run_effect() {
    common::run_reactive(async {
        let t = Trigger::new();
        let runs = Rc::new(RefCell::new(0u32));
        Effect::new({
            let runs = runs.clone();
            move |_: Option<()>| {
                // No t.track() call.
                *runs.borrow_mut() += 1;
            }
        });
        common::flush_effects().await;
        let initial = *runs.borrow();
        t.notify();
        common::flush_effects().await;
        assert_eq!(*runs.borrow(), initial);
    })
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn copies_share_underlying_trigger() {
    common::run_reactive(async {
        let t = Trigger::new();
        let t2 = t;
        let runs = Rc::new(RefCell::new(0u32));
        Effect::new({
            let runs = runs.clone();
            move |_: Option<()>| {
                t.track();
                *runs.borrow_mut() += 1;
            }
        });
        common::flush_effects().await;
        assert_eq!(*runs.borrow(), 1);

        t2.notify();
        common::flush_effects().await;
        assert_eq!(
            *runs.borrow(),
            2,
            "notify via the copy must wake the effect tracking via the original"
        );
    })
    .await;
}

#[test]
fn debug_prints_trigger_marker() {
    let t = Trigger::new();
    let s = format!("{:?}", t);
    assert!(s.contains("Trigger"));
}
