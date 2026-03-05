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
/// | `Unset`      | not specified (internal)| (default sentinel)|
///
/// # TUI specifics
///
/// In a terminal, "pixels" are character cells — `Cells(1)` is the smallest
/// addressable unit (one glyph wide, one line tall).
///
/// `Percent` is relative to the **parent's inner size** (after padding),
/// matching `box-sizing: border-box` semantics.
///
/// `Fill(weight)` distributes remaining space proportionally among siblings.
/// Two children `Fill(1)` + `Fill(2)` share space as 1/3 and 2/3.
///
/// # Examples
///
/// ```rust
/// use oxidui_style::unit::Unit;
/// let w    = Unit::cells(40);    // exactly 40 columns
/// let h    = Unit::percent(50);  // 50% of parent height
/// let flex = Unit::fill(1);      // take 1 share of remaining space
/// let auto = Unit::AUTO;         // size to content
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Unit {
    /// Absolute size in terminal character cells.
    ///
    /// Negative values are valid for offset-style properties
    /// (e.g. `margin: Cells(-1)` to overlap a border) but are
    /// invalid for `width` / `height`.
    Cells(i32),

    /// Percentage of the parent container's inner dimension (0–100).
    ///
    /// Values above 100 are allowed but produce overflow, matching CSS.
    Percent(u8),

    /// Proportional share of remaining space after fixed/percent children.
    ///
    /// The `u16` is the weight relative to sibling `Fill` elements.
    /// `Fill(0)` is treated as `Auto`.
    Fill(u16),

    /// Size to fit the element's content.
    ///
    /// For text nodes: the natural width/height of the text.
    /// For containers: the smallest bounding box of all children.
    Auto,

    /// Property is absent — use inherited value or layout default.
    ///
    /// This is an internal sentinel. User-facing SCSS syntax should
    /// never emit `Unset` directly; the parser produces `None` at the
    /// `Style` field level instead. Exists for `Edges<Unit>` where a
    /// `Unit` must be present but is logically absent.
    Unset,
}

impl Unit {
    // Common constants
    /// `Auto` — size to content.
    pub const AUTO: Self = Self::Auto;
    /// `Unset` — logically absent.
    pub const UNSET: Self = Self::Unset;
    /// `100%` — fill the entire parent dimension.
    pub const FULL: Self = Self::Percent(100);
    /// `50%`  — half the parent dimension.
    pub const HALF: Self = Self::Percent(50);
    /// `0` cells.
    pub const ZERO: Self = Self::Cells(0);
    /// `Fill(1)` — take all remaining space (flex: 1).
    pub const FILL: Self = Self::Fill(1);

    /// Absolute cell-count value.
    pub const fn cells(n: i32) -> Self {
        Self::Cells(n)
    }
    /// Percentage value (0–100).
    pub const fn percent(n: u8) -> Self {
        Self::Percent(n)
    }
    /// Proportional fill with the given weight.
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
        matches!(self, Self::Fill(_) | Self::Auto)
    }

    /// `true` if the value is logically absent.
    pub const fn is_unset(self) -> bool {
        matches!(self, Self::Unset)
    }

    /// Extract `Cells(n)` → `Some(n)`, anything else → `None`.
    pub const fn as_cells(self) -> Option<i32> {
        match self {
            Self::Cells(n) => Some(n),
            _ => None,
        }
    }

    /// Extract `Percent(n)` → `Some(n)`, anything else → `None`.
    pub const fn as_percent(self) -> Option<u8> {
        match self {
            Self::Percent(n) => Some(n),
            _ => None,
        }
    }
}

impl Default for Unit {
    fn default() -> Self {
        Self::Unset
    }
}
