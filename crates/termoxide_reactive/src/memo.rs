//! # Memo — derived value computed lazily
//!
//! [`Memo<T>`] is a value whose computation depends on other signals.
//! It is recomputed only when at least one of its dependencies changes,
//! avoiding unnecessary recomputations (_memoization_).

use reactive_graph::{
    computed::Memo as InnerMemo,
    traits::{Get, GetUntracked, Read, ReadUntracked},
};
use std::fmt;

/// Derived value recomputed lazily.
///
/// The closure provided to [`Memo::new`] is executed once initially,
/// and then only when a signal it depends on changes.
///
/// # Example
///
/// ```no_run
/// use termoxide_reactive::{Signal, Memo, runtime::with_owner};
///
/// with_owner(|| {
///     let price = Signal::new(10.0f64);
///     let qty   = Signal::new(3u32);
///
///     let total = Memo::new(move |_prev| price.get() * qty.get() as f64);
///
///     assert_eq!(total.get(), 30.0);
///     qty.set(5);
///     assert_eq!(total.get(), 50.0);
/// });
/// ```
pub struct Memo<T: Send + Sync + 'static>(pub(crate) InnerMemo<T>);

impl<T: Send + Sync + 'static> Copy for Memo<T> {}
impl<T: Send + Sync + 'static> Clone for Memo<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Clone + Send + Sync + PartialEq + 'static> Memo<T> {
    /// Create a new memo from a reactive closure.
    ///
    /// `f` receives the previously computed value (`None` on the first call)
    /// and must return the new derived value.
    pub fn new(f: impl Fn(Option<T>) -> T + Send + Sync + 'static) -> Self {
        Self(InnerMemo::new(move |prev: Option<&T>| f(prev.cloned())))
    }

    /// Returns a cloned copy of the computed value **registering
    /// a dependency** in the current reactive context.
    pub fn get(&self) -> T {
        self.0.get()
    }

    /// Returns a cloned copy **without creating a dependency**.
    pub fn get_untracked(&self) -> T {
        self.0.get_untracked()
    }

    /// Returns a read guard **registering a dependency**.
    pub fn read(&self) -> impl std::ops::Deref<Target = T> + '_ {
        self.0.read()
    }

    /// Returns a read guard **without creating a dependency**.
    pub fn read_untracked(&self) -> impl std::ops::Deref<Target = T> + '_ {
        self.0.read_untracked()
    }

    /// Direct access to the inner `reactive_graph` memo.
    pub fn inner(&self) -> &InnerMemo<T> {
        &self.0
    }
}

impl<T: fmt::Debug + Clone + Send + Sync + PartialEq + 'static> fmt::Debug for Memo<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let val = self.0.read_untracked();
        f.debug_tuple("Memo").field(&*val).finish()
    }
}

impl<T: fmt::Display + Clone + Send + Sync + PartialEq + 'static> fmt::Display for Memo<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.get_untracked())
    }
}
