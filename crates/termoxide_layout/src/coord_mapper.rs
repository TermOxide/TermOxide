//! Coordinate mapping from floating-point layout values to integer terminal cells.
//!
//! taffy resolves layouts in `f32` space.  Terminals address characters with
//! integer column/row coordinates.  This module bridges the gap by converting
//! a [`taffy::tree::Layout`] into a [`MappedRect`] whose fields are saturating
//! `u16` values safe to pass directly to terminal renderers such as Ratatui.

use taffy::tree::Layout;

// ─────────────────────────────────────────────────────────────────────────── //
//  MappedRect
// ─────────────────────────────────────────────────────────────────────────── //

/// A layout rectangle expressed in integer terminal cell coordinates.
///
/// All fields are `u16` because terminal dimensions (columns and rows) never
/// exceed 65 535 in practice and most TUI crates use `u16` for Rect types.
///
/// Positions are **parent-relative**, matching the coordinate system of
/// `taffy::tree::Layout::location`.
///
/// # Example
///
/// ```rust
/// use termoxide_layout::coord_mapper::{CoordMapper, MappedRect};
/// use taffy::tree::Layout;
///
/// // Suppose taffy computed: x=0.0, y=5.0, w=80.0, h=3.5
/// let layout = Layout::new(); // placeholder
/// let rect = CoordMapper::map(&layout);
/// println!("({}, {}) {}×{}", rect.x, rect.y, rect.width, rect.height);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MappedRect {
    /// Horizontal position in terminal columns, relative to the parent node.
    pub x: u16,
    /// Vertical position in terminal rows, relative to the parent node.
    pub y: u16,
    /// Width in terminal columns.
    pub width: u16,
    /// Height in terminal rows.
    pub height: u16,
}

impl MappedRect {
    /// `true` if both `width` and `height` are zero.
    pub fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Apply a `(dx, dy)` offset and return a new `MappedRect`.
    ///
    /// Used to convert parent-relative coordinates to absolute screen
    /// coordinates by accumulating parent positions as the render tree is
    /// walked top-down.
    ///
    /// Both offsets are saturating additions so overflow never wraps.
    pub fn offset(self, dx: u16, dy: u16) -> Self {
        Self {
            x: self.x.saturating_add(dx),
            y: self.y.saturating_add(dy),
            width: self.width,
            height: self.height,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────── //
//  CoordMapper
// ─────────────────────────────────────────────────────────────────────────── //

/// Converts a [`taffy::tree::Layout`] (floating-point) to a [`MappedRect`]
/// (integer terminal cells).
///
/// # Rounding strategy
///
/// | Value   | Rule                                                        |
/// |---------|-------------------------------------------------------------|
/// | `x`     | **floor** — snap the left edge inward                       |
/// | `y`     | **floor** — snap the top edge inward                        |
/// | `width` | **round** — nearest integer avoids systematic under/over-sizing |
/// | `height`| **round** — same as width                                   |
///
/// All results are clamped to `[0, u16::MAX]`. Negative positions (which
/// taffy can produce for absolutely-positioned nodes) are clamped to `0`.
pub struct CoordMapper;

impl CoordMapper {
    /// Convert a `taffy::tree::Layout` into a [`MappedRect`].
    ///
    /// The input layout's `location` and `size` fields are in `f32` cell
    /// units.  The output is a `u16`-saturated, integer-snapped rectangle
    /// ready for terminal rendering.
    ///
    /// # Example
    ///
    /// ```rust
    /// use termoxide_layout::coord_mapper::CoordMapper;
    /// use taffy::tree::Layout;
    ///
    /// let layout = Layout::new(); // placeholder; populated after LayoutEngine::compute
    /// let rect = CoordMapper::map(&layout);
    /// assert_eq!(rect.x, 0);
    /// ```
    pub fn map(layout: &Layout) -> MappedRect {
        // Floor the origin: move left/up to the nearest whole cell to avoid
        // leaving a gap at the start of the element.
        let x = layout.location.x.floor().max(0.0).min(u16::MAX as f32) as u16;
        let y = layout.location.y.floor().max(0.0).min(u16::MAX as f32) as u16;

        // Round sizes: this minimises the average error across elements.
        let width = layout.size.width.max(0.0).min(u16::MAX as f32).round() as u16;
        let height = layout.size.height.max(0.0).min(u16::MAX as f32).round() as u16;

        MappedRect {
            x,
            y,
            width,
            height,
        }
    }

    /// Convert a `taffy::tree::Layout` to an **absolute** [`MappedRect`] by
    /// adding `parent_origin` to the layout's relative position.
    ///
    /// Call this with the cumulative parent origin while walking the render
    /// tree top-down.
    pub fn map_absolute(layout: &Layout, parent_origin: (u16, u16)) -> MappedRect {
        Self::map(layout).offset(parent_origin.0, parent_origin.1)
    }
}

// ─────────────────────────────────────────────────────────────────────────── //
//  Tests
// ─────────────────────────────────────────────────────────────────────────── //

#[cfg(test)]
mod tests {
    use super::*;
    use taffy::{
        geometry::{Point, Size},
        tree::Layout,
    };

    fn make_layout(x: f32, y: f32, w: f32, h: f32) -> Layout {
        let mut l = Layout::new();
        l.location = Point { x, y };
        l.size = Size {
            width: w,
            height: h,
        };
        l
    }

    #[test]
    fn whole_numbers_pass_through() {
        let l = make_layout(10.0, 5.0, 80.0, 24.0);
        let r = CoordMapper::map(&l);
        assert_eq!(
            r,
            MappedRect {
                x: 10,
                y: 5,
                width: 80,
                height: 24
            }
        );
    }

    #[test]
    fn fractional_position_is_floored() {
        let l = make_layout(1.9, 2.7, 40.0, 10.0);
        let r = CoordMapper::map(&l);
        // x: floor(1.9)=1, y: floor(2.7)=2
        assert_eq!(r.x, 1);
        assert_eq!(r.y, 2);
    }

    #[test]
    fn u16_max_is_respected() {
        let l = make_layout(0.0, 0.0, f32::MAX, f32::MAX);
        let r = CoordMapper::map(&l);
        assert_eq!(r.width, u16::MAX);
        assert_eq!(r.height, u16::MAX);
    }

    #[test]
    fn fractional_size_is_rounded() {
        let l = make_layout(0.0, 0.0, 40.4, 10.6);
        let r = CoordMapper::map(&l);
        // width: round(40.4)=40, height: round(10.6)=11
        assert_eq!(r.width, 40);
        assert_eq!(r.height, 11);
    }

    #[test]
    fn negative_position_clamps_to_zero() {
        let l = make_layout(-5.0, -3.0, 20.0, 5.0);
        let r = CoordMapper::map(&l);
        assert_eq!(r.x, 0);
        assert_eq!(r.y, 0);
    }

    #[test]
    fn offset_adds_parent_origin() {
        let base = MappedRect {
            x: 5,
            y: 3,
            width: 40,
            height: 10,
        };
        let shifted = base.offset(10, 4);
        assert_eq!(
            shifted,
            MappedRect {
                x: 15,
                y: 7,
                width: 40,
                height: 10
            }
        );
    }
}
