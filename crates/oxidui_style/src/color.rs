use std::hash::Hash;

/// A CSS-like color value for terminal output.
///
/// Terminal color support comes in three tiers:
///
/// | Tier   | Type              | Support                                      |
/// |--------|-------------------|----------------------------------------------|
/// | 3/4-bit | [`NamedColor`]   | Universal — every terminal                   |
/// | 8-bit  | [`Color::Indexed`] | xterm-256color and above                    |
/// | 24-bit | [`Color::Rgb`]    | Modern terminals (kitty, iTerm2, WinTerm…)   |
///
/// When converting to `ratatui::style::Color`, degrade gracefully:
/// prefer `Rgb`, fall back to `Indexed`, fall back to `Named`.
///
/// # Examples
///
/// ```rust
/// use oxidui_style::color::{Color, NamedColor};
/// let red   = Color::Named(NamedColor::Red);
/// let coral = Color::rgb(255, 127, 80);
/// let grey  = Color::indexed(240);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Color {
    /// One of the 16 standard ANSI terminal colors.
    ///
    /// The actual shade displayed depends on the user's terminal color scheme
    /// (Dracula, Solarized, etc.) — these are semantic names, not absolutes.
    Named(NamedColor),

    /// A 24-bit RGB "true color" value (r, g, b).
    ///
    /// Requires `COLORTERM=truecolor`. Most modern terminals support this,
    /// but SSH sessions or older emulators may not — fall back to `Indexed`
    /// if you need wider compatibility.
    Rgb(u8, u8, u8),

    /// An index into the xterm 256-color palette (0–255).
    ///
    /// - 0–15:   Standard + bright ANSI colors (mirrors [`NamedColor`])
    /// - 16–231: 6×6×6 RGB color cube
    /// - 232–255: Greyscale ramp from dark to light
    Indexed(u8),

    /// Inherit the color from the nearest ancestor that declares one.
    ///
    /// Equivalent to `color: inherit` in CSS. Children pick up the
    /// container's foreground without repeating the declaration.
    Inherit,

    /// No color — the terminal default shows through.
    ///
    /// For backgrounds: the terminal background color.
    /// For foregrounds: the terminal default text color.
    None,
}

impl Color {
    /// Construct a true-color RGB value.
    ///
    /// `const` so proc_macro output has zero runtime cost:
    /// ```rust
    /// use oxidui_style::color::Color;
    /// const CORAL: Color = Color::rgb(255, 127, 80);
    /// ```
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::Rgb(r, g, b)
    }

    /// Construct a 256-palette indexed color.
    ///
    /// ```rust
    /// use oxidui_style::color::Color;
    /// const MID_GREY: Color = Color::indexed(244);
    /// ```
    pub const fn indexed(i: u8) -> Self {
        Self::Indexed(i)
    }

    /// Parse a `#RRGGBB` hex color at compile time.
    ///
    /// Accepts exactly 7 ASCII bytes (including the leading `#`).
    /// Returns `None` on any malformed input — never panics.
    ///
    /// `const` so the proc_macro can emit:
    /// ```rust
    /// use oxidui_style::color::Color;
    /// const C: Color = Color::from_hex_bytes(b"#ff5f00").unwrap();
    /// ```
    pub const fn from_hex_bytes(bytes: &[u8]) -> Option<Self> {
        match bytes {
            [b'#', r1, r2, g1, g2, b1, b2] => {
                let r = hex_byte(*r1, *r2);
                let g = hex_byte(*g1, *g2);
                let b = hex_byte(*b1, *b2);
                match (r, g, b) {
                    (Some(r), Some(g), Some(b)) => Some(Self::Rgb(r, g, b)),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Returns `true` if this color carries no concrete color information
    /// (`None` or `Inherit`).
    pub const fn is_abstract(self) -> bool {
        matches!(self, Self::None | Self::Inherit)
    }

    /// Convert to `ratatui::style::Color`. Lossy — `Inherit` and `None`
    /// both map to `Reset`.
    #[cfg(feature = "ratatui")]
    pub fn to_ratatui(self) -> ratatui::style::Color {
        match self {
            Self::Named(n) => n.to_ratatui(),
            Self::Rgb(r, g, b) => ratatui::style::Color::Rgb(r, g, b),
            Self::Indexed(i) => ratatui::style::Color::Indexed(i),
            Self::Inherit => ratatui::style::Color::Reset,
            Self::None => ratatui::style::Color::Reset,
        }
    }
}

// ---------------------------------------------------------------------------
// Compile-time hex parsing helpers (private)
// ---------------------------------------------------------------------------

/// Decode one ASCII hex nibble into 0–15. `const`-compatible.
const fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Combine two nibble bytes (`hi`, `lo`) into a full byte.
/// Returns `None` if either nibble is invalid.
const fn hex_byte(hi: u8, lo: u8) -> Option<u8> {
    match (hex_nibble(hi), hex_nibble(lo)) {
        (Some(h), Some(l)) => Some((h << 4) | l),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Named (ANSI) colors
// ---------------------------------------------------------------------------

/// The 16 standard ANSI terminal colors.
///
/// Discriminants match ANSI color indices 0–15, making escape-code
/// generation trivial: `\x1b[30m` = `Black`, `\x1b[31m` = `Red`, …
///
/// The precise RGB rendered is **theme-defined** — these are semantic
/// names, not absolute colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum NamedColor {
    // Normal (ANSI 30–37 fg / 40–47 bg)
    Black = 0,
    Red = 1,
    Green = 2,
    Yellow = 3,
    Blue = 4,
    Magenta = 5,
    Cyan = 6,
    White = 7,

    // Bright (ANSI 90–97 fg / 100–107 bg)
    BrightBlack = 8, // typically rendered as dark grey
    BrightRed = 9,
    BrightGreen = 10,
    BrightYellow = 11,
    BrightBlue = 12,
    BrightMagenta = 13,
    BrightCyan = 14,
    BrightWhite = 15,
}

impl NamedColor {
    /// The ANSI palette index (0–15) for this color.
    pub const fn ansi_index(self) -> u8 {
        self as u8
    }

    #[cfg(feature = "ratatui")]
    pub fn to_ratatui(self) -> ratatui::style::Color {
        use ratatui::style::Color as R;
        match self {
            Self::Black => R::Black,
            Self::Red => R::Red,
            Self::Green => R::Green,
            Self::Yellow => R::Yellow,
            Self::Blue => R::Blue,
            Self::Magenta => R::Magenta,
            Self::Cyan => R::Cyan,
            Self::White => R::White,
            Self::BrightBlack => R::DarkGray,
            Self::BrightRed => R::LightRed,
            Self::BrightGreen => R::LightGreen,
            Self::BrightYellow => R::LightYellow,
            Self::BrightBlue => R::LightBlue,
            Self::BrightMagenta => R::LightMagenta,
            Self::BrightCyan => R::LightCyan,
            Self::BrightWhite => R::Gray,
        }
    }
}
