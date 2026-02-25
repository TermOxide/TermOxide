use super::color::Color;
/// Four-sided shorthand for `padding`, `margin`, border widths, etc.
///
/// Mirrors the CSS shorthand model where a single property expands to
/// four independent sides: top, right, bottom, left.
///
/// # Type parameter
///
/// `T` is most commonly [`super::unit::Unit`] (padding, margin) but can be any `Copy`
/// type — e.g. `Color` for per-side border colors.
///
/// # CSS shorthand mapping
///
/// | CSS shorthand              | `Edges` constructor                              |
/// |----------------------------|--------------------------------------------------|
/// | `padding: 8px`             | `Edges::all(Unit::cells(8))`                     |
/// | `padding: 4px 8px`         | `Edges::symmetric(Unit::cells(4), Unit::cells(8))`|
/// | `padding: 1px 2px 3px 4px` | `Edges::new(c(1), c(2), c(3), c(4))`            |
///
/// # Examples
///
/// ```rust
/// let p = Edges::all(Unit::cells(1));
/// let p = Edges::symmetric(Unit::cells(2), Unit::cells(4));
/// let p = Edges::new(Unit::cells(1), Unit::ZERO, Unit::cells(1), Unit::ZERO);
/// println!("{:?}", p.top);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Edges<T: Copy> {
    /// Top side (`padding-top`, `margin-top`, …).
    pub top: T,
    /// Right side (`padding-right`, `margin-right`, …).
    pub right: T,
    /// Bottom side (`padding-bottom`, `margin-bottom`, …).
    pub bottom: T,
    /// Left side (`padding-left`, `margin-left`, …).
    pub left: T,
}

impl<T: Copy> Edges<T> {
    /// All four sides equal — CSS `padding: 8px`.
    pub const fn all(v: T) -> Self {
        Self {
            top: v,
            right: v,
            bottom: v,
            left: v,
        }
    }

    /// Vertical (top/bottom) and horizontal (left/right) — CSS `padding: 4px 8px`.
    pub const fn symmetric(vertical: T, horizontal: T) -> Self {
        Self {
            top: vertical,
            right: horizontal,
            bottom: vertical,
            left: horizontal,
        }
    }

    /// Each side independently — CSS `padding: top right bottom left`.
    pub const fn new(top: T, right: T, bottom: T, left: T) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    /// Map each side through a function, producing `Edges<U>`.
    ///
    /// ```rust
    /// let raw: Edges<i32> = Edges::all(4);
    /// let units = raw.map(Unit::cells);
    /// ```
    pub fn map<U: Copy, F: Fn(T) -> U>(self, f: F) -> Edges<U> {
        Edges {
            top: f(self.top),
            right: f(self.right),
            bottom: f(self.bottom),
            left: f(self.left),
        }
    }

    /// `true` if all four sides satisfy the predicate.
    pub fn all_satisfy<F: Fn(T) -> bool>(&self, f: F) -> bool {
        f(self.top) && f(self.right) && f(self.bottom) && f(self.left)
    }

    /// Total horizontal extent (`left + right`) in resolved cell counts.
    pub fn horizontal_sum(self) -> i32
    where
        T: Into<i32>,
    {
        self.left.into() + self.right.into()
    }

    /// Total vertical extent (`top + bottom`) in resolved cell counts.
    pub fn vertical_sum(self) -> i32
    where
        T: Into<i32>,
    {
        self.top.into() + self.bottom.into()
    }
}

impl<T: Copy + Default> Default for Edges<T> {
    fn default() -> Self {
        Self::all(T::default())
    }
}

/// A complete border declaration — line style and optional color.
///
/// Combines CSS `border-style` and `border-color`. In a TUI, border
/// "thickness" is binary (present/absent) — one character cell — so there
/// is no `border-width` analogue. Per-side control can be achieved with
/// `Edges<Option<Border>>` if needed.
///
/// # Examples
///
/// ```rust
/// let b = Border::ROUNDED.with_color(Color::Named(NamedColor::Cyan));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Border {
    /// Which Unicode box-drawing character set to use.
    pub style: BorderStyle,

    /// Color override for the border characters.
    ///
    /// `None` = inherit the element's foreground color.
    pub color: Option<Color>,
}

impl Border {
    /// Single thin lines, square corners. No color override.
    pub const SOLID: Self = Self {
        style: BorderStyle::Solid,
        color: None,
    };
    /// Thin lines, rounded corners (`╭ ╮ ╰ ╯`). No color override.
    pub const ROUNDED: Self = Self {
        style: BorderStyle::Rounded,
        color: None,
    };
    /// No border.
    pub const NONE: Self = Self {
        style: BorderStyle::None,
        color: None,
    };

    /// Apply a color override to this border.
    pub const fn with_color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// `true` if the border style is `None` (invisible).
    pub const fn is_none(self) -> bool {
        matches!(self.style, BorderStyle::None)
    }
}

/// Which family of Unicode box-drawing characters to use for a border.
///
/// | Variant   | Characters                      |
/// |-----------|---------------------------------|
/// | `None`    | (no border drawn)               |
/// | `Solid`   | `─ │ ┌ ┐ └ ┘`                  |
/// | `Rounded` | `─ │ ╭ ╮ ╰ ╯`                  |
/// | `Double`  | `═ ║ ╔ ╗ ╚ ╝`                  |
/// | `Thick`   | `━ ┃ ┏ ┓ ┗ ┛`                  |
/// | `Dashed`  | `╌ ╎ ┌ ┐ └ ┘`                  |
///
/// Maps to `ratatui::widgets::BorderType` in the renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BorderStyle {
    /// No border (default).
    #[default]
    None,
    /// Single thin lines, square corners.
    Solid,
    /// Single thin lines, rounded corners — popular in modern TUIs (btop, lazygit).
    Rounded,
    /// Double lines — use for high-emphasis containers like modal dialogs.
    Double,
    /// Thick/bold lines — use for primary focus indicators or selected panels.
    Thick,
    /// Dashed lines — may render as dotted depending on the terminal font.
    Dashed,
}
