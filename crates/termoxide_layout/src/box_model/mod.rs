//! CSS box model types.
//!
//! Every visual element follows the four-layer CSS box model:
//!
//! ```text
//!   ┌─────────────────────────────────────────——┐
//!   │                   margin                  │
//!   │   ┌───────────────────────────────────┐   │
//!   │   │               border              │   │
//!   │   │   ┌───────────────────────────┐   │   │
//!   │   │   │          padding          │   │   │
//!   │   │   │   ┌───────────────────┐   │   │   │
//!   │   │   │   │      content      │   │   │   │
//!   │   │   │   └───────────────────┘   │   │   │
//!   │   │   └───────────────────────────┘   │   │
//!   │   └───────────────────────────────────┘   │
//!   └─────────────────────────────────────────——┘
//! ```
//!
//! ## Module split
//!
//! - [`edges`] — the **common concern** shared by every layer: a four-sided
//!   shorthand container.
//! - [`padding`], [`margin`], [`border`], [`content`] — one module per layer,
//!   each capturing the **specific concerns** that distinguish it from the
//!   others.

pub mod border;
pub mod content;
pub mod edges;
pub mod margin;
pub mod padding;

#[cfg(feature = "future")]
pub mod gap;

pub use border::{Border, BorderStyle, Borders};
pub use content::{BoxSizing, Dimensions, Overflow};
pub use edges::Edges;
pub use margin::Margin;
pub use padding::Padding;

#[cfg(feature = "future")]
pub use gap::Gap;
