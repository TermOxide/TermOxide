//! # Signal — reactive mutable state
//!
//! [`Signal<T>`] is a single-threaded mutable state that automatically
//! notifies all its subscribers (memos, effects) on every mutation.

use reactive_graph::{
    owner::LocalStorage,
    signal::RwSignal as InnerSignal,
    traits::{Get, GetUntracked, Read, ReadUntracked, Set, Update, UpdateUntracked, Write},
};
use std::fmt;

/// Mutable state.
///
/// Any read automatically creates a dependency:
/// the context will be re-run when the signal changes.
///
/// # Example
///
/// ```
/// use termoxide_reactive::{Signal, runtime::with_owner};
///
/// with_owner(|| {
///     let count = Signal::new(0i32);
///
///     count.set(42);
///     assert_eq!(count.get(), 42);
///
///     count.update(|v| *v += 1);
///     assert_eq!(count.get(), 43);
/// });
/// ```
pub struct Signal<T: 'static>(pub(crate) InnerSignal<T, LocalStorage>);

impl<T: 'static> Copy for Signal<T> {}
impl<T: 'static> Clone for Signal<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static> Signal<T> {
    /// Create a new signal with the initial value `value`.
    pub fn new(value: T) -> Self {
        Self(InnerSignal::new_local(value))
    }

    /// Returns a cloned copy of the value **registering a dependency**
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
    pub fn update_untracked(&self, f: impl FnOnce(&mut T)) {
        self.0.update_untracked(f);
    }

    /// Returns a read guard.
    ///
    /// Registers a dependency in the current reactive context.
    pub fn read(&self) -> impl std::ops::Deref<Target = T> + '_ {
        self.0.read()
    }

    /// Returns a read guard **without creating a dependency**.
    pub fn read_untracked(&self) -> impl std::ops::Deref<Target = T> + '_ {
        self.0.read_untracked()
    }

    /// Returns a write guard.
    ///
    /// Subscribers are notified when the guard is released.
    pub fn write(&self) -> impl std::ops::DerefMut<Target = T> + '_ {
        self.0.write()
    }

    pub fn inner(&self) -> &InnerSignal<T, LocalStorage> {
        &self.0
    }
}

impl<T: fmt::Debug + 'static> fmt::Debug for Signal<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let val = self.0.read_untracked();
        f.debug_tuple("Signal").field(&*val).finish()
    }
}

impl<T: fmt::Display + 'static> fmt::Display for Signal<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let val = self.0.read_untracked();
        write!(f, "{}", &*val)
    }
}
