//! # Trigger — manual reactivity trigger
//!
//! [`Trigger`] is a trigger without an associated value. It allows
//! manually notifying reactive subscribers (effects, memos) without
//! storing or exposing data.
//!
//! Useful for invalidating caches, signaling events, or orchestrating
//! conditional recomputations.
//!
//! Based on [`reactive_graph::trigger::ArcTrigger`].

use reactive_graph::{
    signal::Trigger as InnerTrigger,
    traits::{Notify, Track},
};
use std::fmt;

/// Manual reactivity trigger without an associated value.
///
/// # Example
///
/// ```no_run
/// use termoxide_reactive::{Trigger, Effect, runtime::with_owner};
///
/// with_owner(|| {
///     let trigger = Trigger::new();
///
///     let t = trigger.clone();
///     let _effect = Effect::new(move |_prev| {
///         t.track(); // registers the dependency
///         println!("Effect triggered!");
///     });
///
///     trigger.notify(); // forces the effect to re-run
/// });
/// ```
#[derive(Clone)]
pub struct Trigger(pub(crate) InnerTrigger);

impl Trigger {
    /// Create a new trigger.
    pub fn new() -> Self {
        Self(InnerTrigger::new())
    }

    /// Register this trigger as a dependency in the current reactive context.
    ///
    /// Calling [`notify`](Trigger::notify) later will invalidate the context.
    pub fn track(&self) {
        self.0.track();
    }

    /// Notify all reactive contexts that called [`track`](Trigger::track).
    pub fn notify(&self) {
        self.0.notify();
    }

    /// Direct access to the inner `reactive_graph` trigger for advanced usages.
    pub fn inner(&self) -> &InnerTrigger {
        &self.0
    }
}

impl Default for Trigger {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Trigger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Trigger").finish_non_exhaustive()
    }
}
