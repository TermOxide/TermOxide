//! # Effect — reactive side-effect
//!
//! An [`Effect`] is a closure that automatically re-executes whenever a
//! signal it depends on changes.

use reactive_graph::{effect::Effect as InnerEffect, owner::LocalStorage};

/// Side-effect that re-executes when its dependencies change.
///
/// The closure receives the value returned by its previous execution
/// (`None` on the first call), allowing carrying minimal state between runs.
///
/// # Example
///
/// ```no_run
/// use termoxide_reactive::{Effect, Signal, runtime::with_owner};
///
/// with_owner(|| {
///     let name = Signal::new(String::from("Alice"));
///
///     Effect::new(move |_prev| {
///         println!("Hello, {}!", name.get());
///     });
///
///     name.set(String::from("Bob")); // prints "Hello, Bob!"
/// });
/// ```
pub struct Effect(pub(crate) InnerEffect<LocalStorage>);

impl Effect {
    /// Create a new reactive effect from a closure.
    ///
    /// The closure is executed immediately, and then whenever any signals
    /// it read during its previous execution change.
    pub fn new<F, T>(f: F) -> Self
    where
        F: Fn(Option<T>) -> T + 'static,
        T: 'static,
    {
        Self(InnerEffect::new(f))
    }

    pub fn inner(&self) -> &InnerEffect<LocalStorage> { &self.0 }
}
