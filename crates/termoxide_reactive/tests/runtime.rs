//! Behavior tests for `Owner` and `with_owner`.

use termoxide_reactive::runtime::{Owner, with_owner};
use termoxide_reactive::{Signal, StoredValue};

#[test]
fn with_owner_returns_inner_value() {
    let v = with_owner(|| 123u32);
    assert_eq!(v, 123);
}

#[test]
fn primitives_work_inside_with_owner() {
    let v = with_owner(|| {
        let s = Signal::new(1u32);
        s.set(2);
        s.get()
    });
    assert_eq!(v, 2);
}

#[test]
fn owner_with_runs_closure_in_scope() {
    let owner = Owner::new();
    let v = owner.with(|| {
        let s = Signal::new(10i32);
        s.get()
    });
    assert_eq!(v, 10);
}

#[test]
fn explicit_owner_keeps_primitives_alive() {
    let owner = Owner::new();
    owner.set();
    let s = Signal::new(7i32);
    assert_eq!(s.get_untracked(), 7);
    drop(owner);
}

#[test]
fn default_constructs_a_usable_owner() {
    let owner: Owner = Owner::default();
    owner.with(|| {
        let s = Signal::new(1u8);
        assert_eq!(s.get_untracked(), 1);
    });
}

#[test]
#[should_panic(expected = "disposed")]
fn dropping_owner_disposes_stored_values() {
    // `with_owner` disposes its Owner on return, so the StoredValue we
    // let escape is backed by storage that's already gone.
    let stored = with_owner(|| StoredValue::new(1i32));
    let _ = stored.get_value();
}
