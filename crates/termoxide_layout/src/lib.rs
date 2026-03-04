//! termoxide_layout
//!
//! This crate provides the layout layer for TermOxide TUI elements using a
//! CSS Flexbox model powered by the `taffy` crate.
//!
//! ## Module overview
//!
//! | Module           | Responsibility                                                  |
//! |------------------|-----------------------------------------------------------------|
//! | `layout_engine`  | Build a `taffy::TaffyTree`, resolve it, produce `taffy::Layout` |
//! | `coord_mapper`   | Convert `f32` taffy coords to integer terminal cell coords      |
//! | `stylesheet`     | Named registry for sharing / reusing `oxidui_style::Style`s     |
//! | `style_macro`    | `style!` DSL macro for inline style declarations                |

/// Flexbox layout engine wrapping `taffy::TaffyTree`.
///
/// See [`layout_engine::LayoutEngine`] for full documentation.
pub mod layout_engine;

/// Coordinate mapping utilities: `f32` → `(u16, u16)` terminal cell grid.
///
/// See [`coord_mapper::CoordMapper`] and [`coord_mapper::MappedRect`].
pub mod coord_mapper;

/// Named style registry for sharing [`oxidui_style::Style`] values.
///
/// See [`stylesheet::StyleSheet`].
pub mod stylesheet;

/// `style!` convenience macro for inline style construction.
pub mod style_macro;

pub use coord_mapper::{CoordMapper, MappedRect};
pub use layout_engine::{LayoutEngine, LayoutError};
pub use stylesheet::StyleSheet;
