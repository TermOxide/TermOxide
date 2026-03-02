//! # Memo — derived value computed lazily
//!
//! [`ArcMemo<T>`] is a value whose computation depends on other signals.
//! It is recomputed only when at least one of its dependencies changes,
//! avoiding unnecessary recomputations (_memoization_).
//!
//! Based on [`reactive_graph::computed::ArcMemo`].

use reactive_graph::{
    computed::ArcMemo as InnerMemo,
    traits::{Get, GetUntracked, Read, ReadUntracked},
};
use std::fmt;

/// Derived value recomputed lazily.
///
/// The closure provided to [`ArcMemo::new`] is executed once initially,
/// and then only when a signal it depends on changes. Reads of the value
/// inside a reactive context register a dependency on this memo.
///
/// # Example
///
/// ```no_run
/// use termoxide_reactive::{ArcRwSignal, ArcMemo, runtime::with_owner};
///
/// with_owner(|| {
///     let price = ArcRwSignal::new(10.0f64);
///     let qty   = ArcRwSignal::new(3u32);
///
///     let total = ArcMemo::new({
///         let price = price.clone();
///         let qty   = qty.clone();
///         move |_prev| price.get() * qty.get() as f64
///     });
///
///     assert_eq!(total.get(), 30.0);
///     qty.set(5);
///     assert_eq!(total.get(), 50.0);
/// });
/// ```
#[derive(Clone)]
pub struct ArcMemo<T: Send + Sync + 'static>(pub(crate) InnerMemo<T>);

impl<T: Clone + Send + Sync + PartialEq + 'static> ArcMemo<T> {
    /// Create a new memo from a reactive closure.
    ///
    /// `f` receives the previously computed value (`None` on the first call)
    /// and must return the new derived value.
    pub fn new(f: impl Fn(Option<T>) -> T + Send + Sync + 'static) -> Self {
        Self(InnerMemo::new(move |prev: Option<&T>| f(prev.cloned())))
    }
}

impl<T: Clone + Send + Sync + PartialEq + 'static> ArcMemo<T> {
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

    /// Direct access to the inner `reactive_graph` memo for advanced usages.
    pub fn inner(&self) -> &InnerMemo<T> {
        &self.0
    }
}

impl<T: fmt::Debug + Clone + Send + Sync + PartialEq + 'static> fmt::Debug for ArcMemo<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let val = self.0.read_untracked();
        f.debug_tuple("ArcMemo").field(&*val).finish()
    }
}

impl<T: fmt::Display + Clone + Send + Sync + PartialEq + 'static> fmt::Display for ArcMemo<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.get_untracked())
    }
}
