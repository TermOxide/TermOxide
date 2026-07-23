//! Gap — inter-child spacing for flex (and future grid) containers.
//!
//! `Gap` does not sit on any single CSS box-model layer the way padding
//! or margin does. It describes the spacing *between* sibling boxes,
//! computed by the layout engine after the children have been measured.
//! It nonetheless shares padding's spatial invariants: CSS requires a
//! non-negative `<length-percentage>` (or the `normal` keyword, which we
//! collapse to `Cells(0)` for now).

use crate::style::unit::Unit;

/// Inter-child spacing inside a flex or grid container.
/// CSS `gap`.
///
/// # Invariant
///
/// | Input                          | Stored as    |
/// |--------------------------------|--------------|
/// | `Unit::Cells(n)` with `n >= 0` | `Cells(n)`   |
/// | `Unit::Cells(n)` with `n < 0`  | `Cells(0)`   |
/// | `Unit::Percent(p)`             | `Percent(p)` |
/// | `Unit::Auto`                   | `Cells(0)`   |
/// | `Unit::Fill(_)`                | `Cells(0)`   |
///
/// # Examples
///
/// ```rust
/// use termoxide_layout::style::{box_model::Gap, unit::Unit};
///
/// let g = Gap::cells(2);
/// assert_eq!(g.unit(), Unit::cells(2));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Gap(Unit);

/// Normalise a [`Unit`] to a CSS-valid gap value.
const fn css_gap_value(v: Unit) -> Unit {
    match v {
        Unit::Cells(n) => {
            if n < 0 {
                Unit::ZERO
            } else {
                Unit::Cells(n)
            }
        },
        Unit::Percent(_) => v,
        // CSS `gap: normal`resolves to
        // `0` for flex containers — that's what we model here too.
        _ => Unit::ZERO,
    }
}

impl Gap {
    pub const ZERO: Self = Self(Unit::ZERO);

    pub const fn new(v: Unit) -> Self { Self(css_gap_value(v)) }

    pub const fn cells(n: i32) -> Self { Self::new(Unit::Cells(n)) }

    pub const fn percent(p: u8) -> Self { Self(Unit::Percent(p)) }

    /// Borrow the underlying [`Unit`].
    pub const fn unit(self) -> Unit { self.0 }
}

impl From<Unit> for Gap {
    /// Lift a raw [`Unit`] into a `Gap`, normalising the input.
    fn from(v: Unit) -> Self { Self::new(v) }
}
