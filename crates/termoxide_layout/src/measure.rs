//! The leaf measurement contract (ADR D8.4). Width tables, probing and
//! wrapping are deferred (D8.1 to D8.3), so nothing here implements it.
//!
//! `text_wrap` is a Layout property and `text_overflow` a Paint property of
//! [`Style`](crate::style::Style) (ADR D8.6).

/// The space a parent offers a leaf on one axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AvailableSpace {
    /// Exactly this many cells.
    Definite(u16),
    MinContent,
    MaxContent,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Size<T> {
    pub width: T,
    pub height: T,
}

/// A leaf's intrinsic sizes, in cells.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct MeasuredSize {
    pub min_content: Size<u16>,
    pub max_content: Size<u16>,
}

/// How many cells `text` takes on one line of the terminal.
pub trait WidthOracle {
    fn width(&self, text: &str) -> u16;
}

/// A leaf that can size itself from its content.
pub trait Measure {
    fn measure(&self, available: Size<AvailableSpace>, oracle: &dyn WidthOracle) -> MeasuredSize;
}
