//! Border — the visible boundary between padding and margin.
//!
//! Border is the *only* box-model layer with a visual concern of its own:
//!
//! - Its per-side value carries an **appearance** ([`BorderStyle`] + optional
//!   [`Color`]) instead of a spatial [`Unit`](crate::unit::Unit).
//! - In a TUI the **width** is binary — one character cell or nothing — so
//!   there is no `border-width` analogue.
//! - The colour falls back to the element's foreground (via
//!   [`Color::Inherit`] semantics) when left unset.

use super::edges::Edges;
use crate::color::Color;

/// A single side's border appearance — line style and optional colour.
///
/// Combines CSS `border-style` and `border-color`. In a TUI, border
/// "thickness" is binary (present/absent) — one character cell — so there
/// is no `border-width` analogue.
///
/// # Examples
///
/// ```rust
/// use termoxide_layout::box_model::Border;
/// use termoxide_layout::color::{Color, NamedColor};
/// let b = Border::ROUNDED.with_color(Color::Named(NamedColor::Cyan));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Border {
    /// Which Unicode box-drawing character set to use.
    pub style: BorderStyle,

    /// Colour override for the border characters.
    ///
    /// `None` = inherit the element's foreground colour.
    pub color: Option<Color>,
}

impl Border {
    pub const SOLID: Self = Self {
        style: BorderStyle::Solid,
        color: None,
    };
    pub const ROUNDED: Self = Self {
        style: BorderStyle::Rounded,
        color: None,
    };
    pub const NONE: Self = Self {
        style: BorderStyle::None,
        color: None,
    };

    pub const fn with_color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    pub const fn is_none(self) -> bool {
        matches!(self.style, BorderStyle::None)
    }
}

/// Which family of Unicode box-drawing characters to use for a border.
///
/// | Variant   | Characters        |
/// |-----------|-------------------|
/// | `None`    | (no border drawn) |
/// | `Solid`   | `─ │ ┌ ┐ └ ┘`     |
/// | `Rounded` | `─ │ ╭ ╮ ╰ ╯`     |
/// | `Double`  | `═ ║ ╔ ╗ ╚ ╝`     |
/// | `Thick`   | `━ ┃ ┏ ┓ ┗ ┛`     |
/// | `Dashed`  | `╌ ╎ ┌ ┐ └ ┘`     |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BorderStyle {
    #[default]
    None,
    Solid,
    Rounded,
    Double,
    Thick,
    Dashed,
}

/// Four-sided border declaration. CSS `border`.
///
/// # Invariant
///
/// | Input                           | Stored as      |
/// |---------------------------------|----------------|
/// | a styled `Border`               | unchanged      |
/// | a `Border` with `style == None` | `Border::NONE` |
///
/// # Examples
///
/// ```rust
/// use termoxide_layout::box_model::{Border, Borders};
/// use termoxide_layout::color::{Color, NamedColor};
///
/// // Uniform border on all four sides.
/// let b = Borders::all(Border::ROUNDED);
/// assert_eq!(b.edges().top, Border::ROUNDED);
///
/// // Per-side: solid top & bottom, nothing left & right.
/// let b = Borders::symmetric(Border::SOLID, Border::NONE);
/// assert_eq!(b.edges().left, Border::NONE);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Borders(Edges<Border>);

/// Normalise a [`Border`] to a canonical per-side value.
const fn css_border_value(v: Border) -> Border {
    if v.is_none() {
        Border::NONE
    } else {
        v
    }
}

impl Borders {
    pub const NONE: Self = Self(Edges::all(Border::NONE));

    pub const fn all(v: Border) -> Self {
        Self(Edges::all(css_border_value(v)))
    }

    pub const fn symmetric(vertical: Border, horizontal: Border) -> Self {
        Self(Edges::symmetric(
            css_border_value(vertical),
            css_border_value(horizontal),
        ))
    }

    pub const fn new(top: Border, right: Border, bottom: Border, left: Border) -> Self {
        Self(Edges::new(
            css_border_value(top),
            css_border_value(right),
            css_border_value(bottom),
            css_border_value(left),
        ))
    }

    /// Borrow the underlying [`Edges`] for read access.
    pub const fn edges(&self) -> &Edges<Border> {
        &self.0
    }

    /// Consume the wrapper and return the underlying [`Edges`].
    pub const fn into_edges(self) -> Edges<Border> {
        self.0
    }

    /// `true` if every side is [`Border::NONE`] — nothing is drawn anywhere.
    pub const fn is_none(&self) -> bool {
        self.0.top.is_none()
            && self.0.right.is_none()
            && self.0.bottom.is_none()
            && self.0.left.is_none()
    }
}

impl From<Edges<Border>> for Borders {
    /// Lift a raw [`Edges`] into a `Borders`, normalising each side.
    fn from(e: Edges<Border>) -> Self {
        Self::new(e.top, e.right, e.bottom, e.left)
    }
}
