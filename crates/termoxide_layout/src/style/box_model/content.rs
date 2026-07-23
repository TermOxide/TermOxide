//! Content box — the innermost band of the CSS box model.
//!
//! Content is the rectangle that owns some concerns no other box-model
//! layer has:
//!
//! - **Dimensions** — explicit and bounded sizes (`width`, `height`,
//!   `min/max-*`).
//! - **Box sizing** — whether `width`/`height` include the padding and border
//!   (CSS `box-sizing: border-box`) or just the content (`content-box`).
//! - **Overflow** — what happens when content is larger than its bounds.

use crate::style::unit::Unit;

/// Explicit and bounded sizes of the content box.
///
/// Each sub-field is stored as `Option<Unit>` so "not declared" stays
/// distinct from "declared as zero".
///
/// # Invariant
///
/// | Input                          | Stored as     |
/// |--------------------------------|---------------|
/// | `Unit::Cells(n)` with `n >= 0` | `Cells(n)`    |
/// | `Unit::Cells(n)` with `n < 0`  | `Cells(0)`    |
/// | `Unit::Percent(p)`             | `Percent(p)`  |
/// | `Unit::Auto`                   | `Auto`        |
/// | `Unit::Fill(_)` (future)       | `Fill(_)`     |
///
/// # Examples
///
/// ```rust
/// use termoxide_layout::style::{box_model::Dimensions, unit::Unit};
///
/// let d = Dimensions::new()
///     .with_width(Unit::cells(40))
///     .with_max_width(Unit::cells(80));
/// assert_eq!(d.width(), Some(Unit::cells(40)));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Dimensions {
    width: Option<Unit>,
    height: Option<Unit>,
    min_width: Option<Unit>,
    min_height: Option<Unit>,
    max_width: Option<Unit>,
    max_height: Option<Unit>,
}

/// Normalise a [`Unit`] to a CSS-valid dimension side value.
const fn css_dimension_value(v: Unit) -> Unit {
    match v {
        Unit::Cells(n) => {
            if n < 0 {
                Unit::ZERO
            } else {
                Unit::Cells(n)
            }
        },
        _ => v,
    }
}

impl Dimensions {
    pub const fn new() -> Self {
        Self {
            width: None,
            height: None,
            min_width: None,
            min_height: None,
            max_width: None,
            max_height: None,
        }
    }

    pub const fn new_with_size(width: Unit, height: Unit) -> Self {
        Self {
            width: Some(css_dimension_value(width)),
            height: Some(css_dimension_value(height)),
            ..Self::new()
        }
    }

    pub const fn with_width(mut self, v: Unit) -> Self {
        self.width = Some(css_dimension_value(v));
        self
    }

    pub const fn with_height(mut self, v: Unit) -> Self {
        self.height = Some(css_dimension_value(v));
        self
    }

    pub const fn with_min_width(mut self, v: Unit) -> Self {
        self.min_width = Some(css_dimension_value(v));
        self
    }

    pub const fn with_min_height(mut self, v: Unit) -> Self {
        self.min_height = Some(css_dimension_value(v));
        self
    }

    pub const fn with_max_width(mut self, v: Unit) -> Self {
        self.max_width = Some(css_dimension_value(v));
        self
    }

    pub const fn with_max_height(mut self, v: Unit) -> Self {
        self.max_height = Some(css_dimension_value(v));
        self
    }

    pub const fn width(&self) -> Option<Unit> { self.width }

    pub const fn height(&self) -> Option<Unit> { self.height }

    pub const fn min_width(&self) -> Option<Unit> { self.min_width }

    pub const fn min_height(&self) -> Option<Unit> { self.min_height }

    pub const fn max_width(&self) -> Option<Unit> { self.max_width }

    pub const fn max_height(&self) -> Option<Unit> { self.max_height }

    /// `true` if no sub-field is declared.
    pub const fn is_empty(&self) -> bool {
        self.width.is_none()
            && self.height.is_none()
            && self.min_width.is_none()
            && self.min_height.is_none()
            && self.max_width.is_none()
            && self.max_height.is_none()
    }

    /// Merge `other` into `self` per sub-field: each `Some` in `other`
    /// overwrites the matching field in `self`; each `None` is skipped.
    pub fn merge(&mut self, other: &Dimensions) {
        if let Some(v) = other.width {
            self.width = Some(v);
        }
        if let Some(v) = other.height {
            self.height = Some(v);
        }
        if let Some(v) = other.min_width {
            self.min_width = Some(v);
        }
        if let Some(v) = other.min_height {
            self.min_height = Some(v);
        }
        if let Some(v) = other.max_width {
            self.max_width = Some(v);
        }
        if let Some(v) = other.max_height {
            self.max_height = Some(v);
        }
    }
}

/// How declared `width` and `height` are interpreted relative to the box
/// model.
///
/// Initial value (CSS standard): `content-box`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BoxSizing {
    /// `width`/`height` describe the content rectangle only; padding and
    /// border are added on top.
    #[default]
    ContentBox,
    /// `width`/`height` describe the outer rectangle, with padding and
    /// border subtracted from the content area.
    BorderBox,
}

/// What to do when content overflows the element's bounds.
///
/// Initial value (CSS standard): `visible`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Overflow {
    /// Render outside bounds — content paints over siblings in z-order.
    #[default]
    Visible,
    /// Clip content at the element's boundary — nothing renders outside.
    Hidden,
    /// Clip content and reserve space for a scrollbar.
    ///
    /// `Overflow::Scroll` is a declaration of intent — the actual scroll
    /// mechanism is outside the scope of this crate.
    Scroll,
}
