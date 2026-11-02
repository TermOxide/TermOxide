/// A length in the layout (ADR D5.2).
///
/// Holds `f32`, so it is `PartialEq` but not `Eq`.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Unit {
    /// Terminal cells, with no aspect compensation (ADR D5.3).
    Cells(i16),
    /// Percent of the parent, `100.0` being all of it.
    Percent(f32),
    /// A share of the space left on the parent's main axis.
    Fr(f32),
    Auto,
}

impl Unit {
    /// The length in cells. `Percent`, `Fr` and `Auto` give `None`: they stay
    /// unresolved until layout (ADR D5.2).
    pub const fn cells(self) -> Option<i16> {
        match self {
            Unit::Cells(n) => Some(n),
            _ => None,
        }
    }

    /// Clamps a negative length to zero, `NaN` included.
    pub(crate) const fn non_negative(self) -> Self {
        match self {
            Unit::Cells(n) if n < 0 => Unit::Cells(0),
            Unit::Percent(p) => Unit::Percent(p.max(0.0)),
            Unit::Fr(f) => Unit::Fr(f.max(0.0)),
            other => other,
        }
    }
}
