/// A dimensional value — the core type for all spatial CSS properties.
///
/// Used for `width`, `height`, `padding`, `margin`, `gap`, and any other
/// property that describes a size or position in the layout.
///
/// # Variants and CSS analogues
///
/// | Variant      | CSS equivalent          | Example           |
/// |--------------|-------------------------|-------------------|
/// | `Cells(n)`   | `Npx` (1px = 1 cell)    | `width: 40`       |
/// | `Percent(n)` | `N%`                    | `width: 50%`      |
/// | `Fill(w)`    | `Nfr` / `flex: N`       | `width: 1fr`      |
/// | `Auto`       | `auto`                  | `width: auto`     |
///
/// # TUI specifics
///
/// In a terminal, "pixels" are character cells — `Cells(1)` is the smallest
/// addressable unit (one glyph wide, one line tall).
///
/// `Percent` resolves against the **parent's content box**.
/// This is independent of the child's own [`super::box_model::BoxSizing`].
///
/// `Fill(weight)` distributes remaining space proportionally among siblings.
/// Two children `Fill(1)` + `Fill(2)` share space as 1/3 and 2/3.
///
/// # Examples
///
/// ```rust
/// use termoxide_layout::unit::Unit;
///
/// let w    = Unit::cells(40);    // exactly 40 columns
/// let h    = Unit::percent(50);  // 50% of parent height
/// let auto = Unit::AUTO;         // size to content
/// # #[cfg(feature = "future")] {
/// let flex = Unit::fill(1);      // take 1 share of remaining space
/// # }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Unit {
    /// Absolute size in terminal character cells.
    Cells(i32),

    /// Percentage of the parent container's inner dimension (0–100).
    ///
    /// Values above 100 are allowed but produce overflow, matching CSS.
    Percent(u8),

    /// Proportional share of remaining space after fixed/percent children.
    ///
    /// The `u16` is the weight relative to sibling `Fill` elements.
    /// `Fill(0)` is treated as `Auto`.
    #[cfg(feature = "future")]
    Fill(u16),

    /// Size to fit the element's content.
    ///
    /// For text nodes: the natural width/height of the text.
    /// For containers: the smallest bounding box of all children.
    Auto,
}

impl Unit {
    pub const AUTO: Self = Self::Auto;
    pub const FULL: Self = Self::Percent(100);
    pub const HALF: Self = Self::Percent(50);
    pub const ZERO: Self = Self::Cells(0);
    #[cfg(feature = "future")]
    pub const FILL: Self = Self::Fill(1);

    pub const fn cells(n: i32) -> Self {
        Self::Cells(n)
    }

    pub const fn percent(n: u8) -> Self {
        Self::Percent(n)
    }

    #[cfg(feature = "future")]
    pub const fn fill(w: u16) -> Self {
        Self::Fill(w)
    }

    /// `true` if the value is concrete and calculable without layout context
    /// (i.e. `Cells` or `Percent`).
    pub const fn is_definite(self) -> bool {
        matches!(self, Self::Cells(_) | Self::Percent(_))
    }

    /// `true` if the value requires layout context to resolve
    /// (`Fill` needs remaining space; `Auto` needs content size).
    pub const fn is_intrinsic(self) -> bool {
        match self {
            #[cfg(feature = "future")]
            Self::Fill(_) => true,
            Self::Auto => true,
            _ => false,
        }
    }

    pub const fn as_cells(self) -> Option<i32> {
        match self {
            Self::Cells(n) => Some(n),
            _ => None,
        }
    }

    pub const fn as_percent(self) -> Option<u8> {
        match self {
            Self::Percent(n) => Some(n),
            _ => None,
        }
    }
}

impl Default for Unit {
    /// Default is [`Unit::ZERO`].
    ///
    /// `Unit` is not itself a CSS property — its default only matters when
    /// it is used as a *side value* in [`super::box_model::Edges`], which
    /// in turn drives the derived defaults of [`super::box_model::Padding`]
    /// and [`super::box_model::Margin`].
    fn default() -> Self {
        Self::ZERO
    }
}
