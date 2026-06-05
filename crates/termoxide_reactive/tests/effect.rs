//! Behavior tests for `Effect`.

mod common;

use std::{cell::RefCell, rc::Rc};

use termoxide_reactive::{Effect, Signal, runtime::Owner};

#[tokio::test(flavor = "current_thread")]
async fn runs_once_on_creation() {
    common::run_reactive(async {
        let runs = Rc::new(RefCell::new(0u32));
        Effect::new({
            let runs = runs.clone();
            move |_: Option<()>| {
                *runs.borrow_mut() += 1;
            }
        });
        common::flush_effects().await;
        assert_eq!(*runs.borrow(), 1);
    })
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn reruns_when_tracked_signal_changes() {
    common::run_reactive(async {
        let s = Signal::new(0i32);
        let seen = Rc::new(RefCell::new(Vec::<i32>::new()));
        Effect::new({
            let seen = seen.clone();
            move |_: Option<()>| {
                seen.borrow_mut().push(s.get());
            }
        });
        common::flush_effects().await;
        s.set(1);
        common::flush_effects().await;
        s.set(2);
        common::flush_effects().await;
        assert_eq!(*seen.borrow(), vec![0, 1, 2]);
    })
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn does_not_rerun_when_unrelated_signal_changes() {
    common::run_reactive(async {
        let tracked = Signal::new(0i32);
        let untracked = Signal::new(0i32);
        let runs = Rc::new(RefCell::new(0u32));
        Effect::new({
            let runs = runs.clone();
            move |_: Option<()>| {
                tracked.get();
                *runs.borrow_mut() += 1;
            }
        });
        common::flush_effects().await;
        let initial = *runs.borrow();
        untracked.set(99);
        common::flush_effects().await;
        assert_eq!(*runs.borrow(), initial);
    })
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn receives_prev_return_value() {
    common::run_reactive(async {
        let s = Signal::new(0i32);
        let prevs = Rc::new(RefCell::new(Vec::<Option<i32>>::new()));
        Effect::new({
            let prevs = prevs.clone();
            move |prev: Option<i32>| {
                prevs.borrow_mut().push(prev);
                s.get() // becomes next iteration's `prev`
            }
        });
        common::flush_effects().await;
        s.set(10);
        common::flush_effects().await;
        s.set(20);
        common::flush_effects().await;
        assert_eq!(*prevs.borrow(), vec![None, Some(0), Some(10)]);
    })
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn stops_running_after_owner_is_dropped() {
    common::run_reactive(async {
        let s = Signal::new(0i32);
        let runs = Rc::new(RefCell::new(0u32));

        {
            // Inner owner: scopes the lifetime of the Effect only.
            let inner = Owner::new();
            inner.set();
            Effect::new({
                let runs = runs.clone();
                move |_: Option<()>| {
                    s.get();
                    *runs.borrow_mut() += 1;
                }
            });
            common::flush_effects().await;
            assert_eq!(*runs.borrow(), 1);
            drop(inner);
        }

        let before = *runs.borrow();
        s.set(42);
        common::flush_effects().await;
        assert_eq!(s.get_untracked(), 42, "outer signal still works");
        assert_eq!(*runs.borrow(), before, "disposed effect does not re-run");
    })
    .await;
}
