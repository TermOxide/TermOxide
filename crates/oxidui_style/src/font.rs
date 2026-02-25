/// Text modifier flags — bold, italic, underline, etc.
///
/// A bitset rather than an enum, so modifiers compose freely:
/// `FontStyle::BOLD | FontStyle::ITALIC`.
///
/// # Storage
///
/// A single `u8` — 6 bits used, 2 reserved for future extensions.
/// `Copy` + `const`-constructible + `Hash`-able, zero overhead.
///
/// # CSS equivalents
///
/// Combines `font-weight`, `font-style`, `text-decoration`, and several
/// terminal-specific properties (`blink`, `dim`) with no CSS analogue.
///
/// # Examples
///
/// ```rust
/// use oxidui_style::{Style, font::FontStyle};
/// let heading = FontStyle::BOLD | FontStyle::UNDERLINE;
/// assert!(heading.has(FontStyle::BOLD));
/// assert!(!heading.has(FontStyle::ITALIC));
///
/// let style = Style { font_style: Some(FontStyle::BOLD | FontStyle::ITALIC), ..Style::new() };
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FontStyle(pub u8);

impl FontStyle {
    /// No modifiers — plain text.
    pub const NORMAL: Self = Self(0b0000_0000);

    /// Bold / increased weight. Terminal: `\x1b[1m`.
    ///
    /// Some terminals also brighten the foreground color when bold is active.
    pub const BOLD: Self = Self(0b0000_0001);

    /// Italic / oblique. Terminal: `\x1b[3m`.
    ///
    /// Not all terminal fonts render italic — some substitute a color change.
    pub const ITALIC: Self = Self(0b0000_0010);

    /// Underline. Terminal: `\x1b[4m`. Widely supported.
    pub const UNDERLINE: Self = Self(0b0000_0100);

    /// Blinking text. Terminal: `\x1b[5m`.
    ///
    /// Many modern terminals disable blink for accessibility. Use sparingly.
    pub const BLINK: Self = Self(0b0000_1000);

    /// Strikethrough / line-through. Terminal: `\x1b[9m`.
    pub const STRIKETHROUGH: Self = Self(0b0001_0000);

    /// Dim / faint — reduced intensity. Terminal: `\x1b[2m`.
    ///
    /// Useful for de-emphasized text (disabled items, secondary info).
    /// Exact rendering is terminal-dependent.
    pub const DIM: Self = Self(0b0010_0000);

    /// Return a new `FontStyle` with the flags from `other` added.
    pub const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Return a new `FontStyle` with the flags from `other` removed.
    pub const fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    /// `true` if **all** flags in `other` are set in `self`.
    pub const fn has(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// `true` if **any** flag in `other` is set in `self`.
    pub const fn has_any(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    /// `true` if no flags are set — plain, unmodified text.
    pub const fn is_normal(self) -> bool {
        self.0 == 0
    }
}

impl std::ops::BitOr for FontStyle {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        self.with(rhs)
    }
}
impl std::ops::BitAnd for FontStyle {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}
impl std::ops::BitOrAssign for FontStyle {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}
