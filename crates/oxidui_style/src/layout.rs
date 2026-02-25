/// How an element lays out its children. CSS `display`.
///
/// Restricted to values relevant in a TUI context — no `inline`, `table`,
/// or `grid` for now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Display {
    /// Stack children vertically, each on its own line (default).
    ///
    /// Children without an explicit height size to their content.
    #[default]
    Block,

    /// Flexible box layout. Direction controlled by [`FlexDirection`];
    /// alignment by [`Align`] and [`Justify`].
    Flex,

    /// Remove from layout entirely — no space taken, not rendered.
    /// Equivalent to CSS `display: none`.
    None,
}

/// Primary axis of a flex container. CSS `flex-direction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FlexDirection {
    /// Left-to-right (default). Main axis = horizontal.
    #[default]
    Row,
    /// Top-to-bottom. Main axis = vertical.
    Column,
    /// Right-to-left.
    RowReverse,
    /// Bottom-to-top.
    ColumnReverse,
}

impl FlexDirection {
    /// `true` for `Row` or `RowReverse`.
    pub const fn is_horizontal(self) -> bool {
        matches!(self, Self::Row | Self::RowReverse)
    }
    /// `true` for `Column` or `ColumnReverse`.
    pub const fn is_vertical(self) -> bool {
        matches!(self, Self::Column | Self::ColumnReverse)
    }
    /// `true` if the order is reversed.
    pub const fn is_reversed(self) -> bool {
        matches!(self, Self::RowReverse | Self::ColumnReverse)
    }
}

/// Alignment of children along the **cross axis**. CSS `align-items`.
///
/// For a `Row` container the cross axis is vertical; for `Column` it is
/// horizontal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Align {
    /// Pack toward the start of the cross axis.
    Start,
    /// Stretch to fill the cross axis (default).
    ///
    /// A child in a `Row` expands to the full row height unless it has
    /// an explicit height set.
    #[default]
    Stretch,
    /// Center along the cross axis.
    Center,
    /// Pack toward the end of the cross axis.
    End,
    /// Align along text baseline.
    ///
    /// In TUI all cells share the same height, so this is usually
    /// equivalent to `Start`.
    Baseline,
}

/// Distribution of children along the **main axis**. CSS `justify-content`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Justify {
    /// Pack toward the start (default).
    #[default]
    Start,
    /// Center along the main axis.
    Center,
    /// Pack toward the end.
    End,
    /// Equal space **between** children; edges touch the container boundary.
    SpaceBetween,
    /// Equal space **around** each child; edge gaps are half of inner gaps.
    SpaceAround,
    /// Equal space everywhere — before first, between all, after last.
    SpaceEvenly,
}

/// Horizontal text alignment within an element. CSS `text-align`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TextAlign {
    /// Align to the left edge (default for LTR text).
    #[default]
    Left,
    /// Center within the element's width.
    Center,
    /// Align to the right edge.
    Right,
}

/// What to do when content overflows the element's bounds. CSS `overflow`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Overflow {
    /// Render outside bounds — content paints over siblings in z-order (default).
    ///
    /// Use carefully; most containers should use `Hidden`.
    #[default]
    Visible,
    /// Clip content at the element's boundary — nothing renders outside.
    Hidden,
    /// Clip content and show a scrollbar.
    ///
    /// Your framework must manage scroll state separately.
    Scroll,
}
