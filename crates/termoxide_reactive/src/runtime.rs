//! # Runtime — reactive lifetime management
//!
//! The reactive graph from `reactive_graph` is scoped by an [`Owner`].
//! All primitives (signals, memos, effects) must be created inside an
//! active owner to be collected correctly.

use reactive_graph::owner::Owner as InnerOwner;

/// Root of the reactive graph.
///
/// An `Owner` defines the lifetime scope for reactive primitives.
/// When it is dropped, all signals, memos and effects created within its
/// scope are invalidated and freed.
///
/// In production it is recommended to create a single `Owner` at
/// application startup and keep it for the lifetime of the app.
///
/// # Example
///
/// ```rust
/// use termoxide_reactive::runtime::Owner;
/// use termoxide_reactive::ArcRwSignal;
///
/// let owner = Owner::new();
/// let _guard = owner.set(); // activate the owner on the current thread
///
/// let signal = ArcRwSignal::new(42i32);
/// assert_eq!(signal.get(), 42);
/// ```
pub struct Owner(InnerOwner);

impl Owner {
    /// Create a new reactive owner.
    pub fn new() -> Self {
        Self(InnerOwner::new())
    }

    /// Activate this owner on the current thread and return an RAII guard.
    ///
    /// The previous owner is restored when the guard is dropped.
    pub fn set(&self) {
        self.0.set()
    }

    /// Execute `f` within the scope of this owner.
    pub fn with<R>(&self, f: impl FnOnce() -> R) -> R {
        self.0.with(f)
    }
}

impl Default for Owner {
    fn default() -> Self {
        Self::new()
    }
}

/// Execute `f` in a new reactive scope.
///
/// This function is the most concise way to create a temporary owner,
/// suitable for tests and small examples.
///
/// ```rust
/// use termoxide_reactive::{ArcRwSignal, runtime::with_owner};
///
/// with_owner(|| {
///     let x = ArcRwSignal::new(1u32);
///     x.set(2);
///     assert_eq!(x.get(), 2);
/// });
/// ```
pub fn with_owner<R>(f: impl FnOnce() -> R) -> R {
    let owner = Owner::new();
    owner.with(f)
}
