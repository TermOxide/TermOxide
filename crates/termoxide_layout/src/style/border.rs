/// The glyph set of a box's border, one per box (ADR D5.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum BorderStyle {
    Single,
    Double,
    Rounded,
    Thick,
    Ascii,
    None,
}

/// Which sides of a box draw a border. Each drawn side takes one cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BorderSides(u8);

impl BorderSides {
    pub const ALL: Self = Self(0b1111);
    pub const BOTTOM: Self = Self(0b0100);
    pub const LEFT: Self = Self(0b1000);
    pub const RIGHT: Self = Self(0b0010);
    pub const TOP: Self = Self(0b0001);

    pub const fn union(self, other: Self) -> Self { Self(self.0 | other.0) }

    /// `true` if every side of `other` is drawn.
    pub const fn contains(self, other: Self) -> bool { self.0 & other.0 == other.0 }
}
