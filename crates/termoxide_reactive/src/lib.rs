//! # termoxide_reactive — Layer 1: Reactive heart of TermOxide
//!
//! This crate exposes TermOxide's fine-grained reactivity primitives.
//! It wraps the [`reactive_graph`](https://crates.io/crates/reactive_graph)
//! crate (Leptos ecosystem) to provide an idiomatic API consistent with
//! the rest of the framework.
//!
//! ## Overview of primitives
//!
//! | Type | Role |
//! |------|------|
//! | [`ArcRwSignal<T>`] | Thread-safe mutable state with automatic notifications |
//! | [`ArcMemo<T>`] | Derived value computed lazily |
//! | [`Effect`] | Reactive side-effect re-run when dependencies change |
//! | [`Resource<T>`] | Asynchronous loading integrated into the reactive graph |
//! | [`Trigger`] | Manual trigger without an associated value |
//!
//! ## Quickstart
//!
//! ```no_run
//! use termoxide_reactive::{ArcRwSignal, ArcMemo, Effect, Trigger, runtime::with_owner};
//!
//! with_owner(|| {
//!     // 1. Signal — mutable state
//!     let count = ArcRwSignal::new(0i32);
//!
//!     // 2. Memo — derived value
//!     let doubled = ArcMemo::new({
//!         let c = count.clone();
//!         move |_| c.get() * 2
//!     });
//!
//!     // 3. Effect — reactive side-effect
//!     let _fx = Effect::new({
//!         let doubled = doubled.clone();
//!         move |_| println!("doubled = {}", doubled.get())
//!     });
//!
//!     // 4. Trigger — manual invalidation
//!     let trigger = Trigger::new();
//!
//!     count.set(5);
//!     assert_eq!(doubled.get(), 10);
//! });
//! ```
//!
//! ## Runtime management
//!
//! All reactive primitives must be created within the scope of an active
//! [`runtime::Owner`]. Use [`runtime::with_owner`] for tests and small examples,
//! or [`runtime::Owner::new`] + [`runtime::Owner::set`] for long-running
//! applications.
//!
//! For [`Resource<T>`], a Tokio runtime must be available on the current
//! thread (for example via `#[tokio::main]` or `tokio::runtime::Runtime::new()`).

pub mod effect;
pub mod memo;
pub mod resource;
pub mod runtime;
pub mod signal;
pub mod trigger;

// ── Re-exports publics ──────────────────────────────────────────────────────

pub use effect::Effect;
pub use memo::ArcMemo;
pub use resource::{Resource, ResourceState};
pub use runtime::{Owner, with_owner};
pub use signal::ArcRwSignal;
pub use trigger::Trigger;
