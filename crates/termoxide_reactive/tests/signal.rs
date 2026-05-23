//! Behavior tests for `Signal<T>`.

mod common;

use std::cell::RefCell;
use std::rc::Rc;
use termoxide_reactive::{Effect, Signal, runtime::with_owner};

#[test]
fn set_then_get_returns_new_value() {
    with_owner(|| {
        let s = Signal::new(0i32);
        s.set(42);
        assert_eq!(s.get(), 42);
    });
}

#[test]
fn new_signal_exposes_initial_value() {
    with_owner(|| {
        let s = Signal::new(String::from("hello"));
        assert_eq!(s.get(), "hello");
    });
}

#[test]
fn update_mutates_in_place() {
    with_owner(|| {
        let s = Signal::new(10i32);
        s.update(|v| *v += 5);
        assert_eq!(s.get(), 15);
    });
}

#[test]
fn signal_is_copy_and_shares_state() {
    with_owner(|| {
        let s = Signal::new(0i32);
        let c = s;
        c.set(5);
        assert_eq!(s.get(), 5);
    });
}

#[test]
fn debug_formats_with_inner_value() {
    with_owner(|| {
        let s = Signal::new(7i32);
        assert_eq!(format!("{:?}", s), "Signal(7)");
    });
}

#[test]
fn display_formats_with_inner_value() {
    with_owner(|| {
        let s = Signal::new(7i32);
        assert_eq!(format!("{}", s), "7");
    });
}

#[test]
fn read_guard_exposes_value() {
    with_owner(|| {
        let s = Signal::new(99i32);
        let g = s.read_untracked();
        assert_eq!(*g, 99);
    });
}

#[tokio::test(flavor = "current_thread")]
async fn effect_reruns_when_signal_changes_through_set() {
    common::run_reactive(async {
        let runs = Rc::new(RefCell::new(0u32));
        let s = Signal::new(0u32);
        Effect::new({
            let runs = runs.clone();
            move |_: Option<()>| {
                s.get();
                *runs.borrow_mut() += 1;
            }
        });
        common::flush_effects().await;
        assert_eq!(*runs.borrow(), 1, "initial run");

        s.set(1);
        common::flush_effects().await;
        s.set(2);
        common::flush_effects().await;

        assert_eq!(*runs.borrow(), 3);
    })
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn write_guard_notifies_when_dropped() {
    common::run_reactive(async {
        let s = Signal::new(0u32);
        let runs = Rc::new(RefCell::new(0u32));
        Effect::new({
            let runs = runs.clone();
            move |_: Option<()>| {
                s.get();
                *runs.borrow_mut() += 1;
            }
        });
        common::flush_effects().await;
        let initial = *runs.borrow();

        let mut g = s.write();
        *g = 7;
        // Mutation via DerefMut alone must not schedule the subscriber.
        common::flush_effects().await;
        assert_eq!(
            *runs.borrow(),
            initial,
            "guard must defer notification until drop"
        );
        *g = 8;
        drop(g);

        common::flush_effects().await;
        assert_eq!(s.get_untracked(), 8);
        assert_eq!(
            *runs.borrow(),
            initial + 1,
            "drop fires exactly one notification, regardless of mutation count"
        );
    })
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn update_untracked_does_not_notify_subscribers() {
    common::run_reactive(async {
        let runs = Rc::new(RefCell::new(0u32));
        let s = Signal::new(0i32);
        Effect::new({
            let runs = runs.clone();
            move |_: Option<()>| {
                s.get();
                *runs.borrow_mut() += 1;
            }
        });
        common::flush_effects().await;
        let initial = *runs.borrow();
        assert_eq!(initial, 1, "initial run should have fired");

        s.update_untracked(|v| *v = 99);
        common::flush_effects().await;

        assert_eq!(*runs.borrow(), initial);
        assert_eq!(s.get_untracked(), 99);
    })
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn get_untracked_does_not_create_dependency() {
    common::run_reactive(async {
        let runs = Rc::new(RefCell::new(0u32));
        let s = Signal::new(0i32);
        Effect::new({
            let runs = runs.clone();
            move |_: Option<()>| {
                let _ = s.get_untracked();
                *runs.borrow_mut() += 1;
            }
        });
        common::flush_effects().await;
        let initial = *runs.borrow();
        assert_eq!(initial, 1);

        s.set(123);
        common::flush_effects().await;
        assert_eq!(*runs.borrow(), initial);
    })
    .await;
}
