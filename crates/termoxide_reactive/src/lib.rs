//! # termoxide_reactive: Reactive heart of TermOxide
//!
//! This crate exposes TermOxide's reactivity primitives,
//! modelled after React / SolidJS.
//!
//! ## Overview of primitives
//!
//! |Type|Role|
//! |------|------|
//! |[`Signal<T>`]|Mutable reactive state with automatic notifications|
//! |[`Memo<T>`]|Derived value computed lazily, recomputed on dependency change|
//! |[`Effect`]|Reactive side-effect re-run when dependencies change|
//! |[`Resource<T>`]|Asynchronous loading integrated into the reactive graph|
//! |[`Trigger`]|Manual trigger without an associated value|
//! |[`StoredValue<T>`]|Non-reactive, owner-scoped `Copy` storage|
//!
//! All handles ([`Signal`], [`Memo`], [`Trigger`], [`Resource`],
//! [`StoredValue`]) storage is tied to the enclosing [`runtime::Owner`].
//!
//! ## Quickstart
//!
//! ```no_run
//! use termoxide_reactive::{
//!     Effect,
//!     Memo,
//!     Signal,
//!     Trigger,
//!     runtime::with_owner,
//! };
//!
//! with_owner(|| {
//!     let count = Signal::new(0i32);
//!     let doubled = Memo::new(move |_| count.get() * 2);
//!
//!     let trigger = Trigger::new();
//!     Effect::new(move |_| {
//!         trigger.track();
//!         println!("doubled = {}", doubled.get());
//!     });
//!
//!     count.set(5);
//!     trigger.notify();
//!     assert_eq!(doubled.get(), 10);
//! });
//! ```
//!
//! ## Runtime management
//!
//! All reactive primitives must be created within the scope of an active
//! [`runtime::Owner`]. Use [`runtime::with_owner`] for tests and small
//! examples, or [`runtime::Owner::new`] + [`runtime::Owner::set`] for
//! long-running applications.

pub mod effect;
pub mod memo;
pub mod resource;
pub mod runtime;
pub mod signal;
pub mod stored;
pub mod trigger;

// ── Re-exports publics ──────────────────────────────────────────────────────

pub use effect::Effect;
pub use memo::Memo;
pub use resource::{Resource, ResourceState};
pub use runtime::{Owner, with_owner};
pub use signal::Signal;
pub use stored::StoredValue;
pub use trigger::Trigger;
