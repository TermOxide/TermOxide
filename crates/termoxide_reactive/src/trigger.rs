//! # Trigger — manual reactivity trigger
//!
//! [`Trigger`] is a signal without an associated value. It allows
//! manually notifying reactive subscribers (effects, memos) without
//! storing or exposing data.

use std::fmt;

use reactive_graph::{
    signal::Trigger as InnerTrigger,
    traits::{Notify, Track},
};

/// Manual reactivity trigger without an associated value.
///
/// # Example
///
/// ```no_run
/// use termoxide_reactive::{Effect, Trigger, runtime::with_owner};
///
/// with_owner(|| {
///     let trigger = Trigger::new();
///
///     Effect::new(move |_prev| {
///         trigger.track(); // registers the dependency
///         println!("Effect triggered!");
///     });
///
///     trigger.notify(); // forces the effect to re-run
/// });
/// ```
#[derive(Copy, Clone)]
pub struct Trigger(pub(crate) InnerTrigger);

impl Trigger {
    /// Create a new trigger.
    pub fn new() -> Self { Self(InnerTrigger::new()) }

    /// Register this trigger as a dependency in the current reactive context.
    ///
    /// Calling [`notify`](Trigger::notify) later will invalidate the context.
    pub fn track(&self) { self.0.track(); }

    /// Notify all reactive contexts that called [`track`](Trigger::track).
    pub fn notify(&self) { self.0.notify(); }

    pub fn inner(&self) -> &InnerTrigger { &self.0 }
}

impl Default for Trigger {
    fn default() -> Self { Self::new() }
}

impl fmt::Debug for Trigger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Trigger").finish_non_exhaustive()
    }
}
