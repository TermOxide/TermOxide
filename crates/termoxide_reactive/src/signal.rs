//! # Signal — reactive mutable state
//!
//! [`Signal<T>`] is a single-threaded mutable state that automatically
//! notifies all its subscribers (memos, effects) on every mutation.

use std::{fmt, marker::PhantomData};

use reactive_graph::{
    owner::LocalStorage,
    signal::RwSignal as InnerSignal,
    traits::{Get, GetUntracked, Read, ReadUntracked, Set, Update, UpdateUntracked, Write},
};

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
///
/// `R` says whether a change to the signal counts as a change of the app's
/// recorded state: [`Recorded`] for [`Signal::new`], [`Unrecorded`] once
/// [`Signal::unrecorded`] is called on it. It lives in the type so it outlasts
/// a signal rebuilt from its value.
pub struct Signal<T: 'static, R = Recorded>(pub(crate) InnerSignal<T, LocalStorage>, PhantomData<fn() -> R>);

/// A signal whose changes are recorded: the default.
pub enum Recorded {}

/// A signal whose changes aren't recorded on their own, for state that changes
/// too often to be worth it, like a tick count. It still carries its value
/// wherever the app's state goes.
pub enum Unrecorded {}

/// [`Recorded`] or [`Unrecorded`].
pub trait RecordMode: 'static {
    const RECORDED: bool;
}

impl RecordMode for Recorded {
    const RECORDED: bool = true;
}

impl RecordMode for Unrecorded {
    const RECORDED: bool = false;
}

impl<T: 'static, R> Copy for Signal<T, R> {}
impl<T: 'static, R> Clone for Signal<T, R> {
    fn clone(&self) -> Self { *self }
}

// `Signal<T>` is `Signal<T, Recorded>` here: a type's default parameters apply
// where a type is named, so `new` makes recorded signals.
impl<T: 'static> Signal<T> {
    /// Create a new signal with the initial value `value`. It is recorded;
    /// [`unrecorded`](Self::unrecorded) makes it not.
    pub fn new(value: T) -> Self { Self(InnerSignal::new_local(value), PhantomData) }
}

impl<T: 'static, R> Signal<T, R> {
    /// The same signal, its changes no longer recorded on their own:
    /// `Signal::new(0).unrecorded()`.
    pub fn unrecorded(self) -> Signal<T, Unrecorded> { Signal(self.0, PhantomData) }

    /// The same signal, its changes recorded.
    pub fn recorded(self) -> Signal<T, Recorded> { Signal(self.0, PhantomData) }

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
    pub fn set(&self, value: T) { self.0.set(value); }

    /// Modifies the value in place using a closure, then notifies subscribers.
    pub fn update(&self, f: impl FnOnce(&mut T)) { self.0.update(f); }

    /// Modifies the value **without notifying** subscribers.
    pub fn update_untracked(&self, f: impl FnOnce(&mut T)) { self.0.update_untracked(f); }

    /// Returns a read guard.
    ///
    /// Registers a dependency in the current reactive context.
    pub fn read(&self) -> impl std::ops::Deref<Target = T> + '_ { self.0.read() }

    /// Returns a read guard **without creating a dependency**.
    pub fn read_untracked(&self) -> impl std::ops::Deref<Target = T> + '_ { self.0.read_untracked() }

    /// Returns a write guard.
    ///
    /// Subscribers are notified when the guard is released.
    pub fn write(&self) -> impl std::ops::DerefMut<Target = T> + '_ { self.0.write() }

    pub fn inner(&self) -> &InnerSignal<T, LocalStorage> { &self.0 }
}

impl<T: fmt::Debug + 'static, R> fmt::Debug for Signal<T, R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let val = self.0.read_untracked();
        f.debug_tuple("Signal").field(&*val).finish()
    }
}

impl<T: fmt::Display + 'static, R> fmt::Display for Signal<T, R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let val = self.0.read_untracked();
        write!(f, "{}", *val)
    }
}

#[cfg(feature = "serde")]
thread_local! {
    /// Set while [`recorded_only`] runs.
    static RECORDED_ONLY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Runs `serialize` with every [`Unrecorded`] signal serializing as `()`: what
/// it encodes changes only when a recorded signal does. Not meant to be
/// decoded.
#[cfg(feature = "serde")]
pub fn recorded_only<O>(serialize: impl FnOnce() -> O) -> O {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) { RECORDED_ONLY.set(self.0); }
    }
    let _restore = Restore(RECORDED_ONLY.replace(true));
    serialize()
}

/// A signal serializes as its current value, without subscribing anything.
#[cfg(feature = "serde")]
impl<T: serde::Serialize + 'static, R: RecordMode> serde::Serialize for Signal<T, R> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if !R::RECORDED && RECORDED_ONLY.get() {
            return serializer.serialize_unit();
        }
        self.0.read_untracked().serialize(serializer)
    }
}

/// A signal deserializes as a new signal, owned by the current owner, holding
/// the value: subscribers of the serialized signal don't come along.
#[cfg(feature = "serde")]
impl<'de, T: serde::Deserialize<'de> + 'static, R> serde::Deserialize<'de> for Signal<T, R> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(|value| Self(InnerSignal::new_local(value), PhantomData))
    }
}
