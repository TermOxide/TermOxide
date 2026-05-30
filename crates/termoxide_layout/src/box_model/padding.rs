//! Padding — inner spacing between the border and the content box.
//!
//! Padding is the inner band of the CSS box model:
//!
//! ```text
//!   margin → border → padding → content
//! ```
//!
//! Compared to its siblings:
//!
//! - It **forbids negative values** (unlike [`super::margin::Margin`]) — there
//!   is no semantic meaning to "negative inner spacing" in CSS.
//! - It has **no `auto` keyword** (unlike [`super::margin::Margin`]) — the
//!   only intrinsic value for padding is zero.

use super::edges::Edges;
use crate::unit::Unit;

/// Inner spacing between the border and the content. CSS `padding`.
///
/// # Invariant
///
/// | Input                          | Stored as     |
/// |--------------------------------|---------------|
/// | `Unit::Cells(n)` with `n >= 0` | `Cells(n)`    |
/// | `Unit::Cells(n)` with `n < 0`  | `Cells(0)`    |
/// | `Unit::Percent(p)`             | `Percent(p)`  |
/// | `Unit::Auto`                   | `Cells(0)`    |
/// | `Unit::Fill(_)`     (future)   | `Cells(0)`    |
///
/// # Examples
///
/// ```rust
/// use termoxide_layout::box_model::Padding;
/// use termoxide_layout::unit::Unit;
///
/// let p = Padding::all(Unit::cells(1));
/// let p = Padding::symmetric(Unit::cells(1), Unit::cells(2));
/// let p = Padding::new(Unit::cells(1), Unit::cells(2), Unit::cells(1), Unit::cells(2));
/// assert_eq!(p.edges().left, Unit::cells(2));
///
/// // Negative cells are clamped:
/// assert_eq!(Padding::all(Unit::cells(-3)).edges().top, Unit::cells(0));
/// // Intrinsic keywords are not valid padding and collapse to zero:
/// assert_eq!(Padding::all(Unit::AUTO).edges().top, Unit::cells(0));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Padding(Edges);

/// Normalise a [`Unit`] to a CSS-valid padding side value.
const fn css_padding_value(v: Unit) -> Unit {
    match v {
        Unit::Cells(n) => {
            if n < 0 {
                Unit::ZERO
            } else {
                Unit::Cells(n)
            }
        }
        Unit::Percent(_) => v,
        _ => Unit::ZERO,
    }
}

impl Padding {
    pub const ZERO: Self = Self(Edges::all(Unit::ZERO));

    pub const fn all(v: Unit) -> Self {
        Self(Edges::all(css_padding_value(v)))
    }

    pub const fn symmetric(vertical: Unit, horizontal: Unit) -> Self {
        Self(Edges::symmetric(
            css_padding_value(vertical),
            css_padding_value(horizontal),
        ))
    }

    pub const fn new(top: Unit, right: Unit, bottom: Unit, left: Unit) -> Self {
        Self(Edges::new(
            css_padding_value(top),
            css_padding_value(right),
            css_padding_value(bottom),
            css_padding_value(left),
        ))
    }

    /// Borrow the underlying [`Edges`] for read access.
    pub const fn edges(&self) -> &Edges {
        &self.0
    }

    /// Consume the wrapper and return the underlying [`Edges`].
    pub const fn into_edges(self) -> Edges {
        self.0
    }
}

impl From<Edges> for Padding {
    /// Lift a raw [`Edges`] into a `Padding`, normalising each side.
    fn from(e: Edges) -> Self {
        Self::new(e.top, e.right, e.bottom, e.left)
    }
}
