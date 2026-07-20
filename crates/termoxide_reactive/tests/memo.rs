//! Behavior tests for `Memo<T>`.

mod common;

use std::{
    cell::RefCell,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
};

use termoxide_reactive::{Effect, Memo, Signal, runtime::with_owner};

#[test]
fn computes_initial_value() {
    with_owner(|| {
        let a = Signal::new(2i32);
        let b = Signal::new(3i32);
        let sum = Memo::new(move |_| a.get() + b.get());
        assert_eq!(sum.get(), 5);
    });
}

#[test]
fn recomputes_when_dependency_changes() {
    with_owner(|| {
        let a = Signal::new(2i32);
        let doubled = Memo::new(move |_| a.get() * 2);
        assert_eq!(doubled.get(), 4);
        a.set(10);
        assert_eq!(doubled.get(), 20);
    });
}

#[test]
fn depends_on_multiple_signals() {
    with_owner(|| {
        let a = Signal::new(1i32);
        let b = Signal::new(10i32);
        let m = Memo::new(move |_| a.get() + b.get());
        assert_eq!(m.get(), 11);
        a.set(5);
        assert_eq!(m.get(), 15);
        b.set(100);
        assert_eq!(m.get(), 105);
    });
}

#[test]
fn skips_recompute_when_equal_value_set() {
    // PartialEq lets `reactive_graph` short-circuit propagation when the new
    // value equals the previous one. The recompute closure may still run,
    // but downstream memos should not re-execute.
    with_owner(|| {
        let src = Signal::new(1i32);
        let calls = Arc::new(AtomicU32::new(0));
        let upstream = Memo::new(move |_| {
            // Maps any input to a constant — equal output across changes.
            let _ = src.get();
            42i32
        });
        let downstream = Memo::new({
            let calls = calls.clone();
            move |_| {
                calls.fetch_add(1, Ordering::SeqCst);
                upstream.get()
            }
        });
        // Prime.
        assert_eq!(downstream.get(), 42);
        let before = calls.load(Ordering::SeqCst);
        // Change source; upstream still yields 42 → downstream should not
        // re-run.
        src.set(2);
        let _ = downstream.get();
        let after = calls.load(Ordering::SeqCst);
        assert_eq!(
            after, before,
            "downstream must not recompute on equal value"
        );
    });
}

#[test]
fn get_untracked_does_not_subscribe_caller() {
    // Reading via get_untracked should not register a dependency on the memo.
    with_owner(|| {
        let src = Signal::new(0i32);
        let m = Memo::new(move |_| src.get() * 2);

        // Outer memo reads `m` untracked → must not depend on it.
        let outer_calls = Arc::new(AtomicU32::new(0));
        let outer = Memo::new({
            let outer_calls = outer_calls.clone();
            move |_| {
                outer_calls.fetch_add(1, Ordering::SeqCst);
                m.get_untracked()
            }
        });

        let _ = outer.get();
        let n0 = outer_calls.load(Ordering::SeqCst);

        // Changing src changes `m`, but `outer` does not depend on `m`.
        src.set(5);
        let _ = outer.get();
        assert_eq!(
            outer_calls.load(Ordering::SeqCst),
            n0,
            "outer must not recompute when m changes"
        );
    });
}

#[test]
fn read_guard_exposes_value() {
    with_owner(|| {
        let s = Signal::new(7i32);
        let m = Memo::new(move |_| s.get() + 1);
        let g = m.read_untracked();
        assert_eq!(*g, 8);
    });
}

#[test]
fn debug_and_display_use_inner_value() {
    with_owner(|| {
        let s = Signal::new(3i32);
        let m = Memo::new(move |_| s.get() * s.get());
        assert_eq!(format!("{}", m), "9");
        assert_eq!(format!("{:?}", m), "Memo(9)");
    });
}

#[test]
fn prev_argument_carries_last_computed_value() {
    with_owner(|| {
        let src = Signal::new(1i32);
        // Sum-of-inputs accumulator: prev + current.
        let acc =
            Memo::new(move |prev: Option<i32>| prev.unwrap_or(0) + src.get());
        assert_eq!(acc.get(), 1); // None + 1
        src.set(2);
        assert_eq!(acc.get(), 3); // 1 + 2
        src.set(4);
        assert_eq!(acc.get(), 7); // 3 + 4
    });
}

#[tokio::test(flavor = "current_thread")]
async fn memo_propagates_changes_to_subscribed_effect() {
    // Composition check: a signal change should flow through a memo to a
    // subscribed effect via the spawner.
    common::run_reactive(async {
        let src = Signal::new(1i32);
        let doubled = Memo::new(move |_| src.get() * 2);
        let observed = Rc::new(RefCell::new(Vec::<i32>::new()));
        Effect::new({
            let observed = observed.clone();
            move |_: Option<()>| {
                observed.borrow_mut().push(doubled.get());
            }
        });
        common::flush_effects().await;
        assert_eq!(*observed.borrow(), vec![2], "initial run reads memo");

        src.set(5);
        common::flush_effects().await;
        assert_eq!(
            *observed.borrow(),
            vec![2, 10],
            "memo change re-runs the effect"
        );
    })
    .await;
}
