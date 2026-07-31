//! Margin — outer spacing between an element's border and its neighbours.
//!
//! Margin is the outermost band of the CSS box model:
//!
//! ```text
//!   margin → border → padding → content
//! ```
//!
//! Compared to its siblings:
//!
//! - It **permits negative values** (unlike [`super::padding::Padding`]) so elements can be pulled toward or overlap
//!   their neighbours.
//! - It supports the **`auto` keyword** ([`Unit::Auto`]) which lets the layout engine distribute remaining axis space.

use super::edges::Edges;
use crate::style::unit::Unit;

/// Outer spacing between the border and neighbours. CSS `margin`.
///
/// # Invariant
///
/// | Input                       | Stored as     |
/// |-----------------------------|---------------|
/// | `Unit::Cells(n)`            | `Cells(n)`    |
/// | `Unit::Percent(p)`          | `Percent(p)`  |
/// | `Unit::Auto`                | `Auto`        |
/// | `Unit::Fill(_)` (future)    | `Cells(0)`    |
///
/// # Examples
///
/// ```rust
/// use termoxide_layout::style::{box_model::Margin, unit::Unit};
///
/// // Overlap the parent's border by one cell.
/// let m = Margin::all(Unit::cells(-1));
/// assert_eq!(m.edges().top, Unit::cells(-1));
///
/// // Centre horizontally.
/// let m = Margin::symmetric(Unit::ZERO, Unit::AUTO);
/// assert_eq!(m.edges().left, Unit::AUTO);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Margin(Edges);

/// Normalise a [`Unit`] to a CSS-valid margin side value.
const fn css_margin_value(v: Unit) -> Unit {
    match v {
        Unit::Cells(_) | Unit::Percent(_) | Unit::Auto => v,
        #[cfg(feature = "future")]
        _ => Unit::ZERO,
    }
}

impl Margin {
    pub const AUTO: Self = Self(Edges::all(Unit::AUTO));
    pub const ZERO: Self = Self(Edges::all(Unit::ZERO));

    pub const fn all(v: Unit) -> Self { Self(Edges::all(css_margin_value(v))) }

    pub const fn symmetric(vertical: Unit, horizontal: Unit) -> Self {
        Self(Edges::symmetric(css_margin_value(vertical), css_margin_value(horizontal)))
    }

    pub const fn new(top: Unit, right: Unit, bottom: Unit, left: Unit) -> Self {
        Self(Edges::new(
            css_margin_value(top),
            css_margin_value(right),
            css_margin_value(bottom),
            css_margin_value(left),
        ))
    }

    /// Centre horizontally along the main axis.
    pub const fn horizontal_auto(vertical: Unit) -> Self { Self::symmetric(vertical, Unit::AUTO) }

    /// Borrow the underlying [`Edges`] for read access.
    pub const fn edges(&self) -> &Edges { &self.0 }

    /// Consume the wrapper and return the underlying [`Edges`].
    pub const fn into_edges(self) -> Edges { self.0 }
}

impl From<Edges> for Margin {
    /// Lift a raw [`Edges`] into a `Margin`, normalising each side.
    fn from(e: Edges) -> Self { Self::new(e.top, e.right, e.bottom, e.left) }
}
