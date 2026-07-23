/// Coordinate mapping utilities: `f32` → `(u16, u16)` terminal cell grid.
///
/// See [`coord_mapper::CoordMapper`] and [`coord_mapper::MappedRect`].
#[cfg(feature = "future")]
pub mod coord_mapper;

/// Flexbox layout engine wrapping `taffy::TaffyTree`.
///
/// See [`layout_engine::LayoutEngine`] for full documentation.
#[cfg(feature = "future")]
pub mod layout_engine;

#[cfg(feature = "future")]
pub use coord_mapper::{CoordMapper, MappedRect};
#[cfg(feature = "future")]
pub use layout_engine::{
    LayoutEngine, LayoutError, LayoutNode, UiLayoutNode, UiStyleSource,
};

#[cfg(test)]
mod tests {}
