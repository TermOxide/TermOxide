//! # Memo — derived value computed lazily
//!
//! [`Memo<T>`] is a value whose computation depends on other signals.
//! It is recomputed only when at least one of its dependencies changes,
//! avoiding unnecessary recomputations (_memoization_).

use std::fmt;

use reactive_graph::{
    computed::Memo as InnerMemo,
    traits::{Get, GetUntracked, Read, ReadUntracked},
};

/// Derived value recomputed lazily.
///
/// The closure provided to [`Memo::new`] is executed once initially,
/// and then only when a signal it depends on changes.
///
/// # Example
///
/// ```
/// use termoxide_reactive::{Memo, Signal, runtime::with_owner};
///
/// with_owner(|| {
///     let price = Signal::new(10f64);
///     let qty = Signal::new(3u32);
///
///     let total = Memo::new(move |_prev| price.get() * qty.get() as f64);
///
///     assert_eq!(total.get(), 30.0);
///     qty.set(5);
///     assert_eq!(total.get(), 50.0);
/// });
/// ```
///
/// Unlike [`Signal`](crate::signal::Signal) and
/// [`StoredValue`](crate::stored::StoredValue), `Memo<T>` requires
/// `T: Send + Sync`. This is imposed by the underlying `reactive_graph`
/// crate: memos hold their subscriber as `Weak<dyn Subscriber + Send + Sync>`
/// and the recompute closure is bounded by `Send + Sync` — there is no
/// thread-local memo variant to opt out of.
pub struct Memo<T: Send + Sync + 'static>(pub(crate) InnerMemo<T>);

impl<T: Send + Sync + 'static> Copy for Memo<T> {}
impl<T: Send + Sync + 'static> Clone for Memo<T> {
    fn clone(&self) -> Self { *self }
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
    pub fn get(&self) -> T { self.0.get() }

    /// Returns a cloned copy **without creating a dependency**.
    pub fn get_untracked(&self) -> T { self.0.get_untracked() }

    /// Returns a read guard **registering a dependency**.
    pub fn read(&self) -> impl std::ops::Deref<Target = T> + '_ { self.0.read() }

    /// Returns a read guard **without creating a dependency**.
    pub fn read_untracked(&self) -> impl std::ops::Deref<Target = T> + '_ { self.0.read_untracked() }

    /// Direct access to the inner `reactive_graph` memo.
    pub fn inner(&self) -> &InnerMemo<T> { &self.0 }
}

impl<T: fmt::Debug + Clone + Send + Sync + PartialEq + 'static> fmt::Debug for Memo<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let val = self.0.read_untracked();
        f.debug_tuple("Memo").field(&*val).finish()
    }
}

impl<T: fmt::Display + Send + Sync + 'static> fmt::Display for Memo<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let val = self.0.read_untracked();
        write!(f, "{}", *val)
    }
}
