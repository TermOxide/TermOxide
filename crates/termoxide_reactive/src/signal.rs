//! # Signal — thread-shareable reactive mutable state
//!
//! [`ArcRwSignal<T>`] is a thread-safe mutable state that automatically
//! notifies all its subscribers (memos, effects) on every mutation.
//!
//! Based on [`reactive_graph::signal::ArcRwSignal`].

use reactive_graph::{
    signal::ArcRwSignal as InnerSignal,
    traits::{Get, GetUntracked, Read, ReadUntracked, Set, Update, UpdateUntracked, Write},
};
use std::fmt;

/// Thread-shareable mutable state.
///
/// Any read performed inside a reactive context (Effect, Memo)
/// automatically creates a dependency: the context will be re-run
/// when the signal changes.
///
/// # Example
///
/// ```no_run
/// use termoxide_reactive::{ArcRwSignal, runtime::with_owner};
///
/// with_owner(|| {
///     let count = ArcRwSignal::new(0i32);
///
///     count.set(42);
///     assert_eq!(count.get(), 42);
///
///     count.update(|v| *v += 1);
///     assert_eq!(count.get(), 43);
/// });
/// ```
#[derive(Clone)]
pub struct ArcRwSignal<T: Send + Sync + 'static>(pub(crate) InnerSignal<T>);

impl<T: Send + Sync + 'static> ArcRwSignal<T> {
    /// Create a new signal with the initial value `value`.
    pub fn new(value: T) -> Self {
        Self(InnerSignal::new(value))
    }

    /// Returns a cloned copy of the value **registering a dependency**
    /// in the current reactive context.
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.0.get()
    }

    /// Returns a cloned copy of the value **without creating a dependency**.
    pub fn get_untracked(&self) -> T
    where
        T: Clone,
    {
        self.0.get_untracked()
    }

    /// Replaces the value and notifies all subscribers.
    pub fn set(&self, value: T) {
        self.0.set(value);
    }

    /// Modifies the value in place using a closure, then notifies subscribers.
    pub fn update(&self, f: impl FnOnce(&mut T)) {
        self.0.update(f);
    }

    /// Modifies the value **without notifying** subscribers.
    ///
    /// Useful for internal mutations that should not trigger re-renders.
    pub fn update_untracked(&self, f: impl FnOnce(&mut T)) {
        self.0.update_untracked(f);
    }

    /// Returns a read guard (acquires a shared lock).
    ///
    /// Registers a dependency in the current reactive context.
    pub fn read(&self) -> impl std::ops::Deref<Target = T> + '_ {
        self.0.read()
    }

    /// Returns a read guard **without creating a dependency**.
    pub fn read_untracked(&self) -> impl std::ops::Deref<Target = T> + '_ {
        self.0.read_untracked()
    }

    /// Returns a write guard (acquires an exclusive lock).
    ///
    /// Subscribers are notified when the guard is released.
    pub fn write(&self) -> impl std::ops::DerefMut<Target = T> + '_ {
        self.0.write()
    }

    /// Direct access to the inner `reactive_graph` signal for advanced usages.
    pub fn inner(&self) -> &InnerSignal<T> {
        &self.0
    }
}

impl<T: fmt::Debug + Send + Sync + 'static> fmt::Debug for ArcRwSignal<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let val = self.0.read_untracked();
        f.debug_tuple("ArcRwSignal").field(&*val).finish()
    }
}

impl<T: fmt::Display + Clone + Send + Sync + 'static> fmt::Display for ArcRwSignal<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.get_untracked())
    }
}
