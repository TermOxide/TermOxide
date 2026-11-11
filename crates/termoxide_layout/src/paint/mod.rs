//! What paint needs from a node's `presented_style`, with nothing tied to a
//! backend.

mod lower;

pub use lower::{ConcreteColor, PaintStyle, Palette, lower};
