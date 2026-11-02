use crate::style::{BorderStyle, Color, Property, Sem, Style, TextOverflow};

/// A colour a terminal can show. It never holds a symbol (ADR D7.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConcreteColor {
    Rgb(u8, u8, u8),
    /// An entry of the user's terminal palette, kept as such until a terminal
    /// profile can say what it looks like.
    Base16(u8),
}

/// The concrete colour of each semantic role.
///
/// The default uses Base16 entries, so it follows the user's terminal theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub accent: ConcreteColor,
    pub error: ConcreteColor,
    pub warning: ConcreteColor,
    pub surface: ConcreteColor,
    pub muted: ConcreteColor,
}

impl Palette {
    pub const fn get(&self, sem: Sem) -> ConcreteColor {
        match sem {
            Sem::Accent => self.accent,
            Sem::Error => self.error,
            Sem::Warning => self.warning,
            Sem::Surface => self.surface,
            Sem::Muted => self.muted,
        }
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            accent: ConcreteColor::Base16(4),
            error: ConcreteColor::Base16(1),
            warning: ConcreteColor::Base16(3),
            surface: ConcreteColor::Base16(0),
            muted: ConcreteColor::Base16(8),
        }
    }
}

/// Lowers a symbolic colour through `palette`; `Rgb` and `Base16` pass
/// through unchanged (ADR D7.2).
///
/// ```rust
/// use termoxide_layout::{
///     paint::{ConcreteColor, Palette, lower},
///     style::{Color, Sem},
/// };
///
/// let palette = Palette::default();
/// assert_eq!(lower(Color::Semantic(Sem::Error), &palette), ConcreteColor::Base16(1));
/// assert_eq!(lower(Color::Rgb(1, 2, 3), &palette), ConcreteColor::Rgb(1, 2, 3));
/// ```
pub const fn lower(color: Color, palette: &Palette) -> ConcreteColor {
    match color {
        Color::Rgb(r, g, b) => ConcreteColor::Rgb(r, g, b),
        Color::Base16(n) => ConcreteColor::Base16(n),
        Color::Semantic(sem) => palette.get(sem),
    }
}

/// The Paint properties of a node, lowered for drawing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PaintStyle {
    /// `None` is the terminal's default colour.
    pub fg: Option<ConcreteColor>,
    /// `None` is the terminal's default colour.
    pub bg: Option<ConcreteColor>,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub reverse: bool,
    /// `None` when the style leaves it unset.
    pub border_style: Option<BorderStyle>,
    /// `None` when the style leaves it unset.
    pub text_overflow: Option<TextOverflow>,
}

impl PaintStyle {
    /// Lowers a node's `presented_style`; an unset attribute is off.
    pub fn new(presented: &Style, palette: &Palette) -> Self {
        let colour = |c: Property<Color>| c.value().map(|c| lower(c, palette));
        let on = |a: Property<bool>| a.value().unwrap_or(false);
        Self {
            fg: colour(presented.fg),
            bg: colour(presented.bg),
            bold: on(presented.bold),
            dim: on(presented.dim),
            italic: on(presented.italic),
            underline: on(presented.underline),
            strikethrough: on(presented.strikethrough),
            reverse: on(presented.reverse),
            border_style: presented.border_style.value(),
            text_overflow: presented.text_overflow.value(),
        }
    }
}
