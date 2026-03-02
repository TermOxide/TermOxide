//! # Effect — reactive side-effect
//!
//! An [`Effect`] is a closure that automatically re-executes whenever a
//! signal it depends on changes. It is useful for triggering I/O operations,
//! UI updates, or logging.
//!
//! Based on [`reactive_graph::effect::Effect`].

use reactive_graph::effect::Effect as InnerEffect;
use reactive_graph::owner::{LocalStorage, SyncStorage};

/// Side-effect that re-executes when its dependencies change.
///
/// The closure receives the value returned by its previous execution
/// (`None` on the first call), allowing carrying minimal state between runs.
///
/// The effect stays active as long as the [`Effect`] handle is alive.
/// Dropping it stops re-executions.
///
/// # Example
///
/// ```no_run
/// use termoxide_reactive::{ArcRwSignal, Effect, runtime::with_owner};
///
/// with_owner(|| {
///     let name = ArcRwSignal::new(String::from("Alice"));
///
///     let _effect = Effect::new({
///         let name = name.clone();
///         move |_prev| {
///             println!("Hello, {}!", name.get());
///         }
///     });
///
///     name.set(String::from("Bob")); // prints "Hello, Bob!"
/// });
/// ```
#[allow(dead_code)]
pub struct Effect(InnerEffect<LocalStorage>);

impl Effect {
    /// Create a new reactive effect from a closure.
    ///
    /// The closure is executed immediately, and then whenever any signals
    /// it read during its previous execution change.
    ///
    /// The effect is stored in thread-local storage and cannot be sent
    /// across threads. For a thread-safe variant, use [`Effect::new_sync`].
    pub fn new<F, T>(f: F) -> Self
    where
        F: Fn(Option<T>) -> T + 'static,
        T: 'static,
    {
        Self(InnerEffect::new(f))
    }

    /// Create a new reactive effect that can be shared across threads.
    ///
    /// Prefer [`Effect::new`] for effects that don't need to cross thread
    /// boundaries (e.g. UI updates on the main thread).
    pub fn new_sync<F, T>(f: F) -> SyncEffect
    where
        F: Fn(Option<T>) -> T + Send + Sync + 'static,
        T: Send + Sync + 'static,
    {
        SyncEffect(InnerEffect::new_sync(f))
    }
}

/// Thread-safe variant of [`Effect`].
///
/// Created via [`Effect::new_sync`]. Use this only when the effect needs
/// to be triggered from or shared across multiple threads, such as when
/// integrating with background workers.
#[allow(dead_code)]
pub struct SyncEffect(InnerEffect<SyncStorage>);
