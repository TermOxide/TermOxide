/// How an element lays out its children. CSS `display`.
///
/// Restricted to values relevant in a TUI context — no `inline`, `table`,
/// or `grid` for now.
///
/// # Default value
///
/// The CSS standard initial value for `display` is `inline`. This crate
/// does not model inline-level layout (there is no fractional-cell
/// equivalent in a terminal grid), so the default falls through to
/// `Block` — the next most-permissive value the engine *can* render.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Display {
    /// Stack children vertically, each on its own line.
    ///
    /// Default of this enum — see the type-level doc for the rationale.
    /// Children without an explicit height size to their content.
    #[default]
    Block,

    /// Flexible box layout. Direction controlled by [`FlexDirection`];
    /// alignment by [`Align`] and [`Justify`].
    ///
    /// Beyond the Rdmp1 scope: gated by `feature = "future"`.
    #[cfg(feature = "future")]
    Flex,

    /// Remove from layout entirely — no space taken, not rendered.
    /// Equivalent to CSS `display: none`.
    None,
}

// ─────────────────────────────────────────────────────────────────────────── //
//  Flex enums — beyond the Rdmp1 scope, gated by `feature = "future"`.
// ─────────────────────────────────────────────────────────────────────────── //

/// Primary axis of a flex container. CSS `flex-direction`.
#[cfg(feature = "future")]
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

#[cfg(feature = "future")]
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
///
/// # Default value
///
/// The CSS initial value for `align-items` is `normal`, which resolves to
/// `stretch` inside a flex container. We don't model `normal` as a
/// separate variant, so the default is the resolved value: `Stretch`.
#[cfg(feature = "future")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Align {
    /// Pack toward the start of the cross axis.
    Start,
    /// Stretch to fill the cross axis.
    ///
    /// A child in a `Row` expands to the full row height unless it has
    /// an explicit height set. Default of this enum.
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
///
/// # Default value
///
/// The CSS initial value for `justify-content` is `normal`, which resolves
/// to `flex-start` inside a flex container. We don't model `normal` as a
/// separate variant, so the default is the resolved value: `Start`
/// (equivalent to `flex-start`).
#[cfg(feature = "future")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Justify {
    /// Pack toward the start. Default of this enum (equivalent to
    /// `flex-start`).
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
///
/// # Default value
///
/// The CSS initial value for `text-align` is `start`. With this crate's
/// (currently unconditional) left-to-right text flow, `start` ≡ `left`,
/// so the default is `Left`.
///
/// Beyond the Rdmp1 scope: gated by `feature = "future"`.
#[cfg(feature = "future")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TextAlign {
    /// Align to the left edge. Default of this enum (≡ CSS `start` in LTR).
    #[default]
    Left,
    /// Center within the element's width.
    Center,
    /// Align to the right edge.
    Right,
}
