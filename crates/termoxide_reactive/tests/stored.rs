//! Behavior tests for `StoredValue<T>`.

mod common;

use termoxide_reactive::runtime::Owner;
use termoxide_reactive::{Effect, StoredValue, Trigger, runtime::with_owner};

#[test]
fn get_returns_initial_value() {
    with_owner(|| {
        let v = StoredValue::new(42i32);
        assert_eq!(v.get_value(), 42);
    });
}

#[test]
fn set_replaces_value() {
    with_owner(|| {
        let v = StoredValue::new(1u32);
        v.set(99);
        assert_eq!(v.get_value(), 99);
    });
}

#[test]
fn update_mutates_in_place() {
    with_owner(|| {
        let v = StoredValue::new(vec![1, 2, 3]);
        v.update(|xs| xs.push(4));
        assert_eq!(v.get_value(), vec![1, 2, 3, 4]);
    });
}

#[test]
fn with_value_avoids_clone() {
    struct NoClone(i32);
    with_owner(|| {
        let v = StoredValue::new(NoClone(7));
        let n = v.with_value(|inner| inner.0);
        assert_eq!(n, 7);
    });
}

#[test]
fn stored_value_is_copy_and_shares_state() {
    with_owner(|| {
        let v = StoredValue::new(0i32);
        let c = v;
        c.set(5);
        assert_eq!(v.get_value(), 5);
    });
}

#[test]
fn debug_uses_inner_value() {
    with_owner(|| {
        let v = StoredValue::new(123i32);
        assert_eq!(format!("{:?}", v), "StoredValue(123)");
    });
}

#[tokio::test(flavor = "current_thread")]
async fn update_does_not_trigger_reactive_effects() {
    // The whole point of StoredValue: writes are not reactive.
    common::run_reactive(async {
        let stored = StoredValue::new(0u32);
        let trg = Trigger::new();
        Effect::new(move |_: Option<()>| {
            trg.track();
            stored.update(|n| *n += 1);
        });
        common::flush_effects().await;
        // Initial run incremented.
        let after_initial = stored.get_value();
        assert_eq!(after_initial, 1);
        // Updating the stored value should NOT trigger another effect run.
        stored.update(|n| *n += 10);
        common::flush_effects().await;
        assert_eq!(stored.get_value(), 11);
        // Sanity: explicit trigger does cause a rerun.
        trg.notify();
        common::flush_effects().await;
        assert_eq!(stored.get_value(), 12);
    })
    .await;
}

#[test]
fn debug_after_owner_dropped_does_not_panic() {
    // Debug must remain infallible even after disposal — it's relied on
    // by logging, panic formatters, and `dbg!`.
    let stored = termoxide_reactive::runtime::with_owner(|| StoredValue::new(42i32));
    let s = format!("{:?}", stored);
    assert!(s.contains("<disposed>"), "got {s:?}");
}

#[test]
#[should_panic(expected = "disposed")]
fn accessing_after_owner_dropped_panics() {
    let stored;
    {
        let owner = Owner::new();
        owner.set();
        stored = StoredValue::new(7i32);
        drop(owner);
    }
    let _ = stored.get_value();
}
