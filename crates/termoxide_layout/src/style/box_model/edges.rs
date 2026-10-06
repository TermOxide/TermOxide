//! Four-sided shorthand container — the shared *shape* of every box-model
//! layer (padding, margin, per-side borders).
//!
//! [`Edges`] is intentionally narrow: each side is a [`Unit`]
//! (spatial layers — padding, margin) or [`Border`](super::border::Border)
//! (the border layer) qualify,
//! because that is the only kind of value the rest of this crate
//! ever stores in a four-sided slot.
//! Specialised semantics live in the sibling modules
//! but they all reuse this shape via newtype wrappers.

use super::border::Border;
use crate::style::unit::Unit;

/// The set of value types permitted in the four sides of an [`Edges`].
pub trait EdgeValue: sealed::Sealed + Copy {}

mod sealed {
    pub trait Sealed {}
}

impl sealed::Sealed for Unit {}
impl EdgeValue for Unit {}

impl sealed::Sealed for Border {}
impl EdgeValue for Border {}

/// Four-sided shorthand: top, right, bottom, left.
///
/// Mirrors the CSS shorthand expansion `prop: <t> <r> <b> <l>` and is the
/// representation reused by every box-model layer via newtype wrappers. The
/// per-side type `T` is sealed to [`EdgeValue`] and defaults to [`Unit`]
/// (used by padding and margin); the border layer instantiates it as
/// [`Border`](super::border::Border).
///
/// # Examples
///
/// ```rust
/// use termoxide_layout::style::{box_model::Edges, unit::Unit};
///
/// let p = Edges::all(Unit::cells(1));
/// let p = Edges::symmetric(Unit::cells(2), Unit::cells(4));
/// let p = Edges::new(Unit::cells(1), Unit::ZERO, Unit::cells(1), Unit::ZERO);
/// assert_eq!(p.top, Unit::cells(1));
///
/// use termoxide_layout::style::box_model::border::Border;
/// let b = Edges::<Border>::all(Border::SOLID);
/// assert_eq!(b.left, Border::SOLID);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Edges<T: EdgeValue = Unit> {
    pub top: T,
    pub right: T,
    pub bottom: T,
    pub left: T,
}

impl<T: EdgeValue> Edges<T> {
    pub const fn all(v: T) -> Self { Self { top: v, right: v, bottom: v, left: v } }

    pub const fn symmetric(vertical: T, horizontal: T) -> Self {
        Self { top: vertical, right: horizontal, bottom: vertical, left: horizontal }
    }

    pub const fn new(top: T, right: T, bottom: T, left: T) -> Self { Self { top, right, bottom, left } }

    /// Iterate over the four sides in CSS shorthand order
    /// (top, right, bottom, left).
    pub const fn as_array(self) -> [T; 4] { [self.top, self.right, self.bottom, self.left] }
}
