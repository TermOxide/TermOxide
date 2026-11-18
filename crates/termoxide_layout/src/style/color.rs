/// A colour as authored. It stays symbolic until paint lowers it for the
/// terminal (ADR D7.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Color {
    Rgb(u8, u8, u8),
    /// An entry of the user's own 16-colour terminal palette, `0..=15`.
    Base16(u8),
    Semantic(Sem),
}

/// A colour named by its role, resolved against the palette at paint.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Sem {
    Accent,
    Error,
    Warning,
    Surface,
    Muted,
}
