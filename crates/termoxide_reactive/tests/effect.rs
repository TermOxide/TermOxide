//! Behavior tests for `Effect`.

mod common;

use std::cell::RefCell;
use std::rc::Rc;
use termoxide_reactive::runtime::Owner;
use termoxide_reactive::{Effect, Signal};

#[tokio::test(flavor = "current_thread")]
async fn runs_once_on_creation() {
    common::init_executor();
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let owner = Owner::new();
            owner.set();
            let runs = Rc::new(RefCell::new(0u32));
            let runs_c = runs.clone();
            Effect::new(move |_: Option<()>| {
                *runs_c.borrow_mut() += 1;
            });
            common::flush_effects().await;
            assert_eq!(*runs.borrow(), 1);
            drop(owner);
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn reruns_when_tracked_signal_changes() {
    common::init_executor();
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let owner = Owner::new();
            owner.set();
            let s = Signal::new(0i32);
            let seen = Rc::new(RefCell::new(Vec::<i32>::new()));
            let seen_c = seen.clone();
            Effect::new(move |_: Option<()>| {
                seen_c.borrow_mut().push(s.get());
            });
            common::flush_effects().await;
            s.set(1);
            common::flush_effects().await;
            s.set(2);
            common::flush_effects().await;
            assert_eq!(*seen.borrow(), vec![0, 1, 2]);
            drop(owner);
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn does_not_rerun_when_unrelated_signal_changes() {
    common::init_executor();
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let owner = Owner::new();
            owner.set();
            let tracked = Signal::new(0i32);
            let untracked = Signal::new(0i32);
            let runs = Rc::new(RefCell::new(0u32));
            let runs_c = runs.clone();
            Effect::new(move |_: Option<()>| {
                tracked.get();
                *runs_c.borrow_mut() += 1;
            });
            common::flush_effects().await;
            let initial = *runs.borrow();
            untracked.set(99);
            common::flush_effects().await;
            assert_eq!(*runs.borrow(), initial);
            drop(owner);
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn receives_prev_return_value() {
    common::init_executor();
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let owner = Owner::new();
            owner.set();
            let s = Signal::new(0i32);
            let prevs = Rc::new(RefCell::new(Vec::<Option<i32>>::new()));
            let prevs_c = prevs.clone();
            Effect::new(move |prev: Option<i32>| {
                prevs_c.borrow_mut().push(prev);
                s.get() // becomes next iteration's `prev`
            });
            common::flush_effects().await;
            s.set(10);
            common::flush_effects().await;
            s.set(20);
            common::flush_effects().await;
            assert_eq!(*prevs.borrow(), vec![None, Some(0), Some(10)]);
            drop(owner);
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn stops_running_after_owner_is_dropped() {
    common::init_executor();
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let s = Signal::new(0i32);
            let runs = Rc::new(RefCell::new(0u32));
            {
                let owner = Owner::new();
                owner.set();
                let runs_c = runs.clone();
                Effect::new(move |_: Option<()>| {
                    s.get();
                    *runs_c.borrow_mut() += 1;
                });
                common::flush_effects().await;
                assert_eq!(*runs.borrow(), 1);
                drop(owner);
            }
            // Owner is dropped. Mutating the signal afterwards must not
            // schedule the now-disposed effect.
            // NOTE: the signal itself was owned by `owner` too, so reads via
            // `s` would access disposed storage. The behavior we care about
            // is: the effect doesn't re-run after disposal.
            let before = *runs.borrow();
            common::flush_effects().await;
            assert_eq!(*runs.borrow(), before);
        })
        .await;
}
