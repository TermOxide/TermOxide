//! # StoredValue — non-reactive, owner-scoped storage
//!
//! [`StoredValue<T>`] is a `Copy` handle to a value that lives in the
//! current [`Owner`](crate::runtime::Owner)'s arena but does **not**
//! participate in the reactive graph. Reads do not subscribe; writes do
//! not notify.
//!
//! Use it for things that need to be shared across closures and survive
//! across renders — timers, RAII guards, mutable scratch buffers,
//! non-reactive component state.

use reactive_graph::owner::StoredValue as InnerStoredValue;
use reactive_graph::traits::{ReadValue, WriteValue};
use std::fmt;

/// Non-reactive, owner-scoped storage.
///
/// `StoredValue<T>` is a non-reactive signal (like useRef in React).
///
/// # Example
///
/// ```no_run
/// use termoxide_reactive::{StoredValue, Effect, Trigger, runtime::with_owner};
///
/// with_owner(|| {
///     let counter = StoredValue::new(0u32);
///     let trg = Trigger::new();
///
///     Effect::new(move |_prev| {
///         trg.track();
///         // This would cause a loop if counter was a signal
///         counter.update(|c| *c += 1);
///     });
///
///     trg.notify();
///     trg.notify();
///     assert_eq!(counter.get_value(), 2);
/// });
/// ```
pub struct StoredValue<T: Send + Sync + 'static>(InnerStoredValue<T>);

impl<T: Send + Sync + 'static> Copy for StoredValue<T> {}
impl<T: Send + Sync + 'static> Clone for StoredValue<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Send + Sync + 'static> StoredValue<T> {
    /// Store `value` in the current reactive owner's arena.
    pub fn new(value: T) -> Self {
        Self(InnerStoredValue::new(value))
    }

    /// Read the value via a closure (no cloning required).
    pub fn with_value<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        self.0
            .try_read_value()
            .map(|g| f(&g))
            .expect("StoredValue accessed after its owner was disposed")
    }

    /// Mutate the value in place.
    pub fn update(&self, f: impl FnOnce(&mut T)) {
        let mut g = self
            .0
            .try_write_value()
            .expect("StoredValue accessed after its owner was disposed");
        f(&mut g);
    }

    /// Replace the value.
    pub fn set(&self, value: T) {
        self.update(|slot| *slot = value);
    }

    /// Return a clone of the value.
    pub fn get_value(&self) -> T
    where
        T: Clone,
    {
        self.with_value(|v| v.clone())
    }

    pub fn inner(&self) -> &InnerStoredValue<T> {
        &self.0
    }
}

impl<T: fmt::Debug + Send + Sync + 'static> fmt::Debug for StoredValue<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.with_value(|v| f.debug_tuple("StoredValue").field(v).finish())
    }
}
