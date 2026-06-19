//! This module defines the foundational type system for a CSS/SCSS-like
//! styling layer. It is designed to be consumed primarily
//! by proc_macro-generated code
//!
//! ## Design philosophy
//!
//! - **Cheap to copy**: Terminal UIs redraw on every frame. Every type that
//!   will live inside a [`Style`] struct implements `Copy` where possible.
//!
//! - **`const`-constructible**: Proc_macros emit code that runs at compile
//!   time. Where possible, constructors are marked `const` so that static style
//!   definitions have zero runtime cost.
//!
//! - **`Option<T>` for unset style fields**: Distinguishing "not set" from "set
//!   to the default value" is critical for cascade and inheritance. A child
//!   that doesn't set `color` must not reset the parent's `color` to the type
//!   default. Almost every field in [`Style`] is `Option<T>`; the sole
//!   exception is `dimensions`, which is itself a struct of six `Option<Unit>`
//!   sub-fields and therefore doesn't need a second wrapper.
pub mod box_model;
pub mod color;
pub mod font;
pub mod layout;
pub mod unit;

#[cfg(feature = "future")]
pub mod number;
#[cfg(feature = "future")]
pub mod str;

#[cfg(feature = "future")]
use box_model::Gap;
use box_model::{
    Border,
    Borders,
    BoxSizing,
    Dimensions,
    Margin,
    Overflow,
    Padding,
};
use color::Color;
use font::FontStyle;
use layout::Display;
#[cfg(feature = "future")]
use layout::{Align, FlexDirection, Justify, TextAlign};
#[cfg(feature = "future")]
use number::{Float, Opacity};
use unit::Unit;

/// A complete set of style declarations for one UI element.
///
/// Most fields are `Option<T>`. `None` means **"not declared on this
/// element"**. This distinction drives three core behaviours:
///
/// 1. **Cascade / inheritance** — a child's `None` field never resets a
///    parent's value. Only `Some(x)` is an active declaration.
///
/// 2. **Style merging** — theme + component + inline styles are applied in
///    priority order via [`Style::merge`]. Later `Some` values win; `None`
///    values are silently skipped.
///
/// 3. **Proc_macro output** — `scss! { color: red; }` generates a `Style` with
///    only `color` set to `Some`. Every other field is `None`.
///
/// # Creating styles
///
/// ```rust
/// use termoxide_layout::style::{
///     Style,
///     box_model::Dimensions,
///     color::{Color, NamedColor},
///     font::FontStyle,
///     unit::Unit,
/// };
///
/// // Direct struct construction
/// let s = Style {
///     dimensions: Dimensions::new()
///         .with_width(Unit::percent(100))
///         .with_height(Unit::cells(3)),
///     background: Some(Color::Named(NamedColor::Blue)),
///     font_style: Some(FontStyle::BOLD),
///     ..Style::new()
/// };
///
/// // Builder pattern
/// let s = Style::new()
///     .with_width(Unit::FULL)
///     .with_background(Color::Named(NamedColor::Blue));
/// ```
///
/// # Merging
///
/// ```rust
/// use termoxide_layout::style::{
///     Style,
///     color::{Color, NamedColor},
/// };
///
/// let mut base = Style {
///     color: Some(Color::Named(NamedColor::White)),
///     ..Style::new()
/// };
/// let over = Style {
///     color: Some(Color::Named(NamedColor::Red)),
///     ..Style::new()
/// };
/// base.merge(&over);
/// assert_eq!(base.color, Some(Color::Named(NamedColor::Red)));
/// ```
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Style {
    // -----------------------------------------------------------------------
    // Box model
    // -----------------------------------------------------------------------
    /// Content-box dimensions (width, height, min/max). See
    /// [`box_model::Dimensions`].
    ///
    /// Unlike the other box-model fields this is not wrapped in `Option`:
    /// `Dimensions` already carries one `Option<Unit>` per sub-field, and a
    /// double `Option` would offer no extra information.
    pub dimensions: Dimensions,

    /// Inner spacing between border and content. CSS `padding`.
    ///
    /// `Padding::all(Unit::cells(1))` = 1-cell padding on every side. See
    /// [`box_model::Padding`] for the full constructor set.
    pub padding: Option<Padding>,

    /// Outer spacing between border and neighbours. CSS `margin`.
    ///
    /// Supports negative cell values (for overlap effects) and
    /// [`Unit::AUTO`](crate::unit::Unit::AUTO) for centring. See
    /// [`box_model::Margin`].
    pub margin: Option<Margin>,

    /// Border appearance, per side. CSS `border`.
    ///
    /// Stored as [`box_model::Borders`] — a four-sided newtype over
    /// [`box_model::Edges`] (like `Margin`/`Padding`) whose per-side value is
    /// a [`box_model::Border`]. Drawn as Unicode box-drawing characters,
    /// always 1 cell thick.
    pub border: Option<Borders>,

    /// How declared dimensions are interpreted relative to padding and
    /// border. CSS `box-sizing`. See [`box_model::BoxSizing`].
    pub box_sizing: Option<BoxSizing>,

    /// Content overflow behaviour. CSS `overflow`. See
    /// [`box_model::Overflow`].
    pub overflow: Option<Overflow>,

    // -----------------------------------------------------------------------
    // Layout
    // -----------------------------------------------------------------------
    /// How children are laid out. CSS `display`.
    pub display: Option<Display>,
    /// Main axis for flex layout. Only meaningful when `display == Flex`.
    /// Beyond the Rdmp1 scope: `feature = "future"`.
    #[cfg(feature = "future")]
    pub flex_direction: Option<FlexDirection>,
    /// Grow factor relative to flex siblings. CSS `flex-grow`. Beyond
    /// Rdmp1: `feature = "future"`.
    #[cfg(feature = "future")]
    pub flex_grow: Option<Float>,
    /// Shrink factor when space is tight. CSS `flex-shrink`. Beyond Rdmp1:
    /// `feature = "future"`.
    #[cfg(feature = "future")]
    pub flex_shrink: Option<Float>,
    /// Cross-axis child alignment. CSS `align-items`. Beyond Rdmp1:
    /// `feature = "future"`.
    #[cfg(feature = "future")]
    pub align_items: Option<Align>,
    /// Main-axis child distribution. CSS `justify-content`. Beyond Rdmp1:
    /// `feature = "future"`.
    #[cfg(feature = "future")]
    pub justify_content: Option<Justify>,
    /// Space between children (not at edges). CSS `gap`.
    ///
    /// Stored as [`box_model::Gap`], which self-validates: negative cells
    /// and intrinsic keywords are normalised to zero at construction.
    /// Beyond Rdmp1: `feature = "future"`.
    #[cfg(feature = "future")]
    pub gap: Option<Gap>,

    // -----------------------------------------------------------------------
    // Visuals
    // -----------------------------------------------------------------------
    /// Foreground (text) colour. CSS `color`.
    ///
    /// Inherited by children that don't declare their own `color`.
    pub color: Option<Color>,

    /// Background fill colour. CSS `background-color`.
    ///
    /// Paints behind the content and padding out to the border edge,
    /// matching the CSS initial `background-clip: border-box`.
    pub background: Option<Color>,

    /// Element opacity. CSS `opacity`. Beyond Rdmp1:
    /// `feature = "future"`.
    ///
    /// Stored as [`number::Opacity`], which clamps input to `[0.0, 1.0]`
    /// at construction — the CSS-legal range. In TUI, implemented as
    /// dimming (`FontStyle::DIM`) rather than alpha-blending; values are
    /// typically quantized to visible/dim/hidden.
    #[cfg(feature = "future")]
    pub opacity: Option<Opacity>,

    // -----------------------------------------------------------------------
    // Typography
    // -----------------------------------------------------------------------
    /// Horizontal text alignment. CSS `text-align`. Beyond Rdmp1:
    /// `feature = "future"`.
    #[cfg(feature = "future")]
    pub text_align: Option<TextAlign>,

    /// Text modifiers — bold, italic, underline, etc.
    ///
    /// Combine with `|`: `FontStyle::BOLD | FontStyle::ITALIC`.
    pub font_style: Option<FontStyle>,
}

impl Style {
    /// All-`None` style — no declarations, the "tabula rasa".
    ///
    /// `const` so it can be used in static contexts:
    /// ```rust
    /// use termoxide_layout::style::Style;
    /// const EMPTY: Style = Style::new();
    /// ```
    pub const fn new() -> Self {
        Self {
            dimensions: Dimensions::new(),
            padding: None,
            margin: None,
            border: None,
            box_sizing: None,
            overflow: None,
            display: None,
            #[cfg(feature = "future")]
            flex_direction: None,
            #[cfg(feature = "future")]
            flex_grow: None,
            #[cfg(feature = "future")]
            flex_shrink: None,
            #[cfg(feature = "future")]
            align_items: None,
            #[cfg(feature = "future")]
            justify_content: None,
            #[cfg(feature = "future")]
            gap: None,
            color: None,
            background: None,
            #[cfg(feature = "future")]
            opacity: None,
            #[cfg(feature = "future")]
            text_align: None,
            font_style: None,
        }
    }

    // -----------------------------------------------------------------------
    // Cascade / merge
    // -----------------------------------------------------------------------

    /// Merge `other` on top of `self` in place.
    ///
    /// For each field: `other`'s `Some(v)` overwrites `self`; `other`'s
    /// `None` leaves `self` unchanged. Implements CSS cascade semantics —
    /// higher-priority (later) declarations win; absence never resets.
    ///
    /// # Example
    ///
    /// ```rust
    /// use termoxide_layout::style::{
    ///     Style,
    ///     color::{Color, NamedColor},
    /// };
    /// let mut s = Style {
    ///     color: Some(Color::Named(NamedColor::White)),
    ///     ..Style::new()
    /// };
    /// s.merge(&Style {
    ///     color: Some(Color::Named(NamedColor::Red)),
    ///     ..Style::new()
    /// });
    /// // s.color == Some(Red)
    /// ```
    pub fn merge(&mut self, other: &Style) {
        // All Option<T> values are Copy here, so we never need `.clone()`.
        macro_rules! m {
            ($f:ident) => {
                if let Some(v) = other.$f {
                    self.$f = Some(v);
                }
            };
        }
        // Dimensions has private sub-fields with their own per-field merge
        // so a child that only declares `width` does not blank the parent's
        // `height`. Delegate to that.
        self.dimensions.merge(&other.dimensions);
        m!(padding);
        m!(margin);
        m!(border);
        m!(box_sizing);
        m!(overflow);
        m!(display);
        #[cfg(feature = "future")]
        {
            m!(flex_direction);
            m!(flex_grow);
            m!(flex_shrink);
            m!(align_items);
            m!(justify_content);
            m!(gap);
        }
        m!(color);
        m!(background);
        #[cfg(feature = "future")]
        m!(opacity);
        #[cfg(feature = "future")]
        m!(text_align);
        m!(font_style);
    }

    /// Non-mutating merge — returns a new `Style` without touching `self`.
    pub fn merged_with(&self, other: &Style) -> Style {
        let mut r = self.clone();
        r.merge(other);
        r
    }

    // -----------------------------------------------------------------------
    // Builder API
    // -----------------------------------------------------------------------
    //
    // Proc_macro output constructs `Style { field: Some(v), ..Style::new() }`
    // directly, which is more efficient. The builder methods below are for
    // ergonomic handwritten code and tests.

    pub fn with_dimensions(mut self, v: Dimensions) -> Self {
        self.dimensions = v;
        self
    }

    pub fn with_width(mut self, v: Unit) -> Self {
        self.dimensions = self.dimensions.with_width(v);
        self
    }

    pub fn with_height(mut self, v: Unit) -> Self {
        self.dimensions = self.dimensions.with_height(v);
        self
    }

    pub fn with_min_width(mut self, v: Unit) -> Self {
        self.dimensions = self.dimensions.with_min_width(v);
        self
    }

    pub fn with_min_height(mut self, v: Unit) -> Self {
        self.dimensions = self.dimensions.with_min_height(v);
        self
    }

    pub fn with_max_width(mut self, v: Unit) -> Self {
        self.dimensions = self.dimensions.with_max_width(v);
        self
    }

    pub fn with_max_height(mut self, v: Unit) -> Self {
        self.dimensions = self.dimensions.with_max_height(v);
        self
    }

    pub fn with_padding(mut self, v: Padding) -> Self {
        self.padding = Some(v);
        self
    }

    pub fn with_margin(mut self, v: Margin) -> Self {
        self.margin = Some(v);
        self
    }

    /// Convenience: uniform padding on all four sides.
    pub fn with_padding_all(self, v: Unit) -> Self {
        self.with_padding(Padding::all(v))
    }

    /// Convenience: uniform margin on all four sides.
    pub fn with_margin_all(self, v: Unit) -> Self {
        self.with_margin(Margin::all(v))
    }

    pub fn with_display(mut self, v: Display) -> Self {
        self.display = Some(v);
        self
    }

    #[cfg(feature = "future")]
    pub fn with_flex_direction(mut self, v: FlexDirection) -> Self {
        self.flex_direction = Some(v);
        self
    }

    #[cfg(feature = "future")]
    pub fn with_flex_grow(mut self, v: Float) -> Self {
        self.flex_grow = Some(v);
        self
    }

    #[cfg(feature = "future")]
    pub fn with_flex_shrink(mut self, v: Float) -> Self {
        self.flex_shrink = Some(v);
        self
    }

    #[cfg(feature = "future")]
    pub fn with_align_items(mut self, v: Align) -> Self {
        self.align_items = Some(v);
        self
    }

    #[cfg(feature = "future")]
    pub fn with_justify_content(mut self, v: Justify) -> Self {
        self.justify_content = Some(v);
        self
    }

    /// Set the inter-child gap. The input is normalised via
    /// [`Gap::new`] (negatives clamped to zero, intrinsic keywords
    /// collapse to zero).
    #[cfg(feature = "future")]
    pub fn with_gap(mut self, v: Unit) -> Self {
        self.gap = Some(Gap::new(v));
        self
    }

    pub fn with_color(mut self, v: Color) -> Self {
        self.color = Some(v);
        self
    }

    pub fn with_background(mut self, v: Color) -> Self {
        self.background = Some(v);
        self
    }

    pub fn with_border(mut self, v: Borders) -> Self {
        self.border = Some(v);
        self
    }

    /// Convenience: uniform border on all four sides.
    pub fn with_border_all(self, v: Border) -> Self {
        self.with_border(Borders::all(v))
    }

    /// Set the element opacity. The input is clamped to `[0.0, 1.0]`
    /// via [`Opacity::from`].
    #[cfg(feature = "future")]
    pub fn with_opacity(mut self, v: Float) -> Self {
        self.opacity = Some(Opacity::from(v));
        self
    }

    #[cfg(feature = "future")]
    pub fn with_text_align(mut self, v: TextAlign) -> Self {
        self.text_align = Some(v);
        self
    }

    pub fn with_font_style(mut self, v: FontStyle) -> Self {
        self.font_style = Some(v);
        self
    }

    pub fn with_overflow(mut self, v: Overflow) -> Self {
        self.overflow = Some(v);
        self
    }

    pub fn with_box_sizing(mut self, v: BoxSizing) -> Self {
        self.box_sizing = Some(v);
        self
    }

    // -----------------------------------------------------------------------
    // Introspection
    // -----------------------------------------------------------------------

    /// `true` if no fields are set (all `None`).
    pub fn is_empty(&self) -> bool { *self == Style::default() }

    /// `true` if any dimension or spacing field is set.
    pub fn has_layout(&self) -> bool {
        let base = !self.dimensions.is_empty()
            || self.padding.is_some()
            || self.margin.is_some();
        #[cfg(feature = "future")]
        let base = base || self.gap.is_some();
        base
    }

    /// `true` if any visual (non-layout) field is set.
    pub fn has_visuals(&self) -> bool {
        let base = self.color.is_some()
            || self.background.is_some()
            || self.border.is_some()
            || self.font_style.is_some();
        #[cfg(feature = "future")]
        let base = base || self.opacity.is_some() || self.text_align.is_some();
        base
    }
}

/// Named style registry for sharing [`crate::Style`] values.
///
/// See [`stylesheet::StyleSheet`].
#[cfg(feature = "future")]
pub mod stylesheet;

#[cfg(feature = "future")]
pub use stylesheet::StyleSheet;

#[cfg(test)]
mod tests {
    #[cfg(feature = "future")]
    use std::borrow::Cow;

    use box_model::{Border, BorderStyle, Borders, Edges, Margin, Padding};
    use color::{Color, NamedColor};
    use font::FontStyle;
    #[cfg(feature = "future")]
    use number::{Float, Int};
    use unit::Unit;

    #[cfg(feature = "future")]
    use super::str::Str;
    use super::*;

    // --- Color ---

    #[cfg(feature = "future")]
    #[test]
    fn color_hex_valid() {
        assert_eq!(
            Color::from_hex_bytes(b"#ff5f00"),
            Some(Color::Rgb(255, 95, 0))
        );
        assert_eq!(
            Color::from_hex_bytes(b"#000000"),
            Some(Color::Rgb(0, 0, 0))
        );
        assert_eq!(
            Color::from_hex_bytes(b"#FFFFFF"),
            Some(Color::Rgb(255, 255, 255))
        );
        assert_eq!(
            Color::from_hex_bytes(b"#aAbBcC"),
            Some(Color::Rgb(0xAA, 0xBB, 0xCC))
        );
    }

    #[cfg(feature = "future")]
    #[test]
    fn color_hex_invalid() {
        // no #
        assert_eq!(Color::from_hex_bytes(b"ff5f00"), None);
        // bad nibble
        assert_eq!(Color::from_hex_bytes(b"#ff5fgg"), None);
        // shorthand not supported
        assert_eq!(Color::from_hex_bytes(b"#fff"), None);
        assert_eq!(Color::from_hex_bytes(b""), None);
    }

    #[test]
    fn color_is_abstract() {
        assert!(Color::None.is_abstract());
        assert!(Color::Inherit.is_abstract());
        assert!(!Color::Named(NamedColor::Red).is_abstract());
        #[cfg(feature = "future")]
        assert!(!Color::rgb(0, 0, 0).is_abstract());
    }

    // --- Int ---

    #[cfg(feature = "future")]
    #[test]
    fn int_arithmetic() {
        assert_eq!(Int::new(3) + Int::new(4), Int::new(7));
        assert_eq!(Int::new(10) - Int::new(3), Int::new(7));
        assert_eq!(-Int::new(5), Int::new(-5));
    }

    #[cfg(feature = "future")]
    #[test]
    fn int_predicates() {
        assert!(Int::ZERO.is_zero());
        assert!(!Int::ONE.is_zero());
        assert!(Int::new(-1).is_negative());
        assert!(!Int::ONE.is_negative());
    }

    // --- Float ---

    #[cfg(feature = "future")]
    #[test]
    fn float_eq_bitwise() {
        assert_eq!(Float::new(1.0), Float::new(1.0));
        assert_ne!(Float::new(0.5), Float::new(0.9));
        let nan = Float::new(f32::NAN);
        assert_eq!(nan, nan); // NaN == NaN via bits — intentional
    }

    #[cfg(feature = "future")]
    #[test]
    fn float_clamp_unit() {
        assert_eq!(Float::new(1.5).clamp_unit(), Float::new(1.0));
        assert_eq!(Float::new(-0.5).clamp_unit(), Float::new(0.0));
        assert_eq!(Float::new(0.75).clamp_unit(), Float::new(0.75));
    }

    #[cfg(feature = "future")]
    #[test]
    fn float_ops() {
        assert_eq!(Float::new(0.5) + Float::new(0.25), Float::new(0.75));
        assert_eq!(Float::new(2.0) * Float::new(3.0), Float::new(6.0));
    }

    // --- Str ---

    #[cfg(feature = "future")]
    #[test]
    fn str_static_is_borrowed() {
        let s = Str::from_static("mono");
        assert!(matches!(s.0, Cow::Borrowed(_)));
        assert_eq!(s.as_str(), "mono");
    }

    #[cfg(feature = "future")]
    #[test]
    fn str_from_string_is_owned() {
        let s = Str::from_string("runtime".to_string());
        assert!(matches!(s.0, Cow::Owned(_)));
    }

    #[cfg(feature = "future")]
    #[test]
    fn str_equality_ignores_cow_variant() {
        assert_eq!(Str::from_static("hello"), Str::from_string("hello".into()));
    }

    // --- Unit ---

    #[test]
    fn unit_predicates() {
        assert!(Unit::cells(10).is_definite());
        assert!(Unit::percent(50).is_definite());
        assert!(!Unit::AUTO.is_definite());

        assert!(Unit::AUTO.is_intrinsic());
        assert!(!Unit::ZERO.is_intrinsic());

        #[cfg(feature = "future")]
        {
            assert!(!Unit::fill(1).is_definite());
            assert!(Unit::fill(1).is_intrinsic());
        }
    }

    #[test]
    fn unit_extractors() {
        assert_eq!(Unit::cells(42).as_cells(), Some(42));
        assert_eq!(Unit::percent(75).as_percent(), Some(75));
        assert_eq!(Unit::AUTO.as_cells(), None);
        assert_eq!(Unit::cells(5).as_percent(), None);
    }

    // --- Edges ---

    #[test]
    fn edges_all() {
        let e = Edges::all(Unit::cells(4));
        assert_eq!(e.top, Unit::cells(4));
        assert_eq!(e.left, Unit::cells(4));
    }

    #[test]
    fn edges_symmetric() {
        let e = Edges::symmetric(Unit::cells(2), Unit::cells(4));
        assert_eq!(e.top, Unit::cells(2));
        assert_eq!(e.right, Unit::cells(4));
    }

    #[test]
    fn edges_as_array() {
        let e = Edges::new(
            Unit::cells(1),
            Unit::cells(2),
            Unit::cells(3),
            Unit::cells(4),
        );
        assert_eq!(e.as_array(), [
            Unit::cells(1),
            Unit::cells(2),
            Unit::cells(3),
            Unit::cells(4),
        ]);
    }

    // --- Padding invariant: CSS forbids negatives and intrinsic keywords ---

    #[test]
    fn padding_clamps_negative_cells() {
        let p = Padding::all(Unit::cells(-5));
        assert_eq!(p.edges().top, Unit::cells(0));
        assert_eq!(p.edges().left, Unit::cells(0));
    }

    #[test]
    fn padding_normalises_auto_to_zero() {
        let p = Padding::all(Unit::AUTO);
        assert_eq!(p.edges().top, Unit::cells(0));
    }

    #[test]
    fn padding_preserves_non_negative_cells_and_percent() {
        let p = Padding::new(
            Unit::cells(1),
            Unit::percent(50),
            Unit::cells(0),
            Unit::percent(25),
        );
        assert_eq!(p.edges().top, Unit::cells(1));
        assert_eq!(p.edges().right, Unit::percent(50));
        assert_eq!(p.edges().bottom, Unit::cells(0));
        assert_eq!(p.edges().left, Unit::percent(25));
    }

    #[test]
    fn padding_from_edges_normalises() {
        // Bypassing the constructor via From must still apply the invariant.
        let raw = Edges::new(
            Unit::cells(-10),
            Unit::AUTO,
            Unit::percent(75),
            Unit::cells(2),
        );
        let p: Padding = raw.into();
        assert_eq!(p.edges().top, Unit::cells(0)); // clamped
        assert_eq!(p.edges().right, Unit::cells(0)); // Auto → 0
        assert_eq!(p.edges().bottom, Unit::percent(75));
        assert_eq!(p.edges().left, Unit::cells(2));
    }

    #[cfg(feature = "future")]
    #[test]
    fn padding_normalises_fill_to_zero() {
        assert_eq!(Padding::all(Unit::fill(2)).edges().top, Unit::cells(0));
    }

    // --- Margin invariant: negatives allowed, intrinsics rejected ---

    #[test]
    fn margin_allows_negative_cells() {
        let m = Margin::all(Unit::cells(-3));
        assert_eq!(m.edges().top, Unit::cells(-3));
    }

    #[test]
    fn margin_preserves_auto() {
        let m = Margin::symmetric(Unit::ZERO, Unit::AUTO);
        assert_eq!(m.edges().left, Unit::AUTO);
        assert_eq!(m.edges().top, Unit::ZERO);
    }

    #[test]
    fn margin_from_edges_normalises() {
        let raw = Edges::new(
            Unit::cells(-1),
            Unit::AUTO,
            Unit::percent(10),
            Unit::cells(0),
        );
        let m: Margin = raw.into();
        assert_eq!(m.edges().top, Unit::cells(-1));
        assert_eq!(m.edges().right, Unit::AUTO);
        assert_eq!(m.edges().bottom, Unit::percent(10));
        assert_eq!(m.edges().left, Unit::cells(0));
    }

    #[cfg(feature = "future")]
    #[test]
    fn margin_normalises_fill_to_zero() {
        assert_eq!(Margin::all(Unit::fill(2)).edges().top, Unit::cells(0));
    }

    // --- Dimensions invariant: CSS forbids negative width/height ---

    #[test]
    fn dimensions_clamps_negative_cells() {
        let d = Dimensions::new()
            .with_width(Unit::cells(-5))
            .with_height(Unit::cells(-3))
            .with_min_width(Unit::cells(-1))
            .with_max_height(Unit::cells(-7));
        assert_eq!(d.width(), Some(Unit::cells(0)));
        assert_eq!(d.height(), Some(Unit::cells(0)));
        assert_eq!(d.min_width(), Some(Unit::cells(0)));
        assert_eq!(d.max_height(), Some(Unit::cells(0)));
    }

    #[test]
    fn dimensions_preserves_valid_values() {
        let d = Dimensions::new()
            .with_width(Unit::percent(50))
            .with_height(Unit::cells(20))
            .with_min_width(Unit::AUTO);
        assert_eq!(d.width(), Some(Unit::percent(50)));
        assert_eq!(d.height(), Some(Unit::cells(20)));
        assert_eq!(d.min_width(), Some(Unit::AUTO));
    }

    #[test]
    fn dimensions_merge_per_field() {
        let mut base = Dimensions::new().with_width(Unit::cells(80));
        let over = Dimensions::new().with_height(Unit::cells(24));
        base.merge(&over);
        assert_eq!(base.width(), Some(Unit::cells(80)));
        assert_eq!(base.height(), Some(Unit::cells(24)));
    }

    // --- Gap invariant (future-gated) ---

    #[cfg(feature = "future")]
    #[test]
    fn gap_clamps_negative_cells() {
        assert_eq!(box_model::Gap::cells(-4).unit(), Unit::cells(0));
    }

    #[cfg(feature = "future")]
    #[test]
    fn gap_normalises_intrinsic_keywords() {
        assert_eq!(box_model::Gap::new(Unit::AUTO).unit(), Unit::cells(0));
        assert_eq!(box_model::Gap::new(Unit::fill(2)).unit(), Unit::cells(0));
    }

    #[cfg(feature = "future")]
    #[test]
    fn gap_preserves_valid_values() {
        assert_eq!(box_model::Gap::cells(3).unit(), Unit::cells(3));
        assert_eq!(box_model::Gap::percent(25).unit(), Unit::percent(25));
    }

    // --- Opacity invariant (future-gated) ---

    #[cfg(feature = "future")]
    #[test]
    fn opacity_clamps_out_of_range() {
        assert_eq!(number::Opacity::new(1.5).get(), 1.0);
        assert_eq!(number::Opacity::new(-0.2).get(), 0.0);
        assert_eq!(number::Opacity::new(0.75).get(), 0.75);
    }

    #[cfg(feature = "future")]
    #[test]
    fn opacity_endpoints() {
        assert_eq!(number::Opacity::TRANSPARENT.get(), 0.0);
        assert_eq!(number::Opacity::OPAQUE.get(), 1.0);
    }

    #[cfg(feature = "future")]
    #[test]
    fn opacity_nan_resolves_to_zero() {
        // f32::clamp resolves NaN to the lower bound — the type is
        // guaranteed never to carry NaN.
        let o = number::Opacity::new(f32::NAN);
        assert_eq!(o.get(), 0.0);
    }

    // --- FontStyle ---

    #[test]
    fn font_style_combine() {
        let s = FontStyle::BOLD | FontStyle::ITALIC;
        assert!(s.has(FontStyle::BOLD));
        assert!(s.has(FontStyle::ITALIC));
        assert!(!s.has(FontStyle::UNDERLINE));
    }

    #[test]
    fn font_style_remove() {
        let s =
            (FontStyle::BOLD | FontStyle::ITALIC).without(FontStyle::ITALIC);
        assert!(s.has(FontStyle::BOLD));
        assert!(!s.has(FontStyle::ITALIC));
    }

    #[test]
    fn font_style_is_normal() {
        assert!(FontStyle::NORMAL.is_normal());
        assert!(!FontStyle::BOLD.is_normal());
        assert!(FontStyle::BOLD.without(FontStyle::BOLD).is_normal());
    }

    // --- Border ---

    #[test]
    fn border_is_none() {
        assert!(Border::NONE.is_none());
        assert!(!Border::SOLID.is_none());
    }

    #[test]
    fn border_with_color() {
        let b = Border::ROUNDED.with_color(Color::Named(NamedColor::Cyan));
        assert_eq!(b.style, BorderStyle::Rounded);
        assert_eq!(b.color, Some(Color::Named(NamedColor::Cyan)));
    }

    // --- Borders: four-sided newtype, same shape as Margin/Padding ---

    #[test]
    fn borders_all_and_symmetric() {
        let b = Borders::all(Border::ROUNDED);
        assert_eq!(b.edges().top, Border::ROUNDED);
        assert_eq!(b.edges().left, Border::ROUNDED);

        let b = Borders::symmetric(Border::SOLID, Border::NONE);
        assert_eq!(b.edges().top, Border::SOLID);
        assert_eq!(b.edges().bottom, Border::SOLID);
        assert_eq!(b.edges().left, Border::NONE);
        assert_eq!(b.edges().right, Border::NONE);
    }

    #[test]
    fn borders_none_is_empty() {
        assert!(Borders::NONE.is_none());
        assert!(Borders::default().is_none());
        assert!(!Borders::all(Border::SOLID).is_none());
    }

    #[test]
    fn borders_canonicalises_styleless_color() {
        // A colour on a `None` side is meaningless and is dropped.
        let stray = Border::NONE.with_color(Color::Named(NamedColor::Red));
        assert_eq!(Borders::all(stray).edges().top, Border::NONE);
    }

    #[test]
    fn borders_from_edges_normalises() {
        // Bypassing the constructor via From must still apply the invariant.
        let raw = Edges::new(
            Border::SOLID,
            Border::NONE.with_color(Color::Named(NamedColor::Red)),
            Border::ROUNDED,
            Border::NONE,
        );
        let b: Borders = raw.into();
        assert_eq!(b.edges().top, Border::SOLID);
        assert_eq!(b.edges().right, Border::NONE); // colour dropped
        assert_eq!(b.edges().bottom, Border::ROUNDED);
        assert_eq!(b.edges().left, Border::NONE);
    }

    // --- Style merge ---

    #[test]
    fn merge_some_wins() {
        let mut base = Style {
            color: Some(Color::Named(NamedColor::White)),
            background: Some(Color::Named(NamedColor::Black)),
            ..Style::new()
        };
        base.merge(&Style {
            color: Some(Color::Named(NamedColor::Red)),
            ..Style::new()
        });
        // overridden
        assert_eq!(base.color, Some(Color::Named(NamedColor::Red)));
        // untouched
        assert_eq!(base.background, Some(Color::Named(NamedColor::Black)));
    }

    #[test]
    fn merge_none_does_not_overwrite() {
        let mut base = Style {
            dimensions: Dimensions::new().with_width(Unit::cells(80)),
            ..Style::new()
        };
        base.merge(&Style::new());
        assert_eq!(base.dimensions.width(), Some(Unit::cells(80)));
    }

    #[test]
    fn merge_dimensions_per_field() {
        // Sibling dimensions should merge per sub-field — declaring `height`
        // on the overlay must not blank the base's `width`.
        let mut base = Style {
            dimensions: Dimensions::new().with_width(Unit::cells(80)),
            ..Style::new()
        };
        base.merge(&Style {
            dimensions: Dimensions::new().with_height(Unit::cells(24)),
            ..Style::new()
        });
        assert_eq!(base.dimensions.width(), Some(Unit::cells(80)));
        assert_eq!(base.dimensions.height(), Some(Unit::cells(24)));
    }

    #[test]
    fn merged_with_is_non_mutating() {
        let base = Style {
            color: Some(Color::Named(NamedColor::White)),
            ..Style::new()
        };
        let merged = base.merged_with(&Style {
            color: Some(Color::Named(NamedColor::Red)),
            ..Style::new()
        });
        // untouched
        assert_eq!(base.color, Some(Color::Named(NamedColor::White)));
        assert_eq!(merged.color, Some(Color::Named(NamedColor::Red)));
    }

    #[test]
    fn style_is_empty() {
        assert!(Style::new().is_empty());
        assert!(!Style::new().with_color(Color::None).is_empty());
    }

    #[test]
    fn builder_chain() {
        let s = Style::new()
            .with_width(Unit::FULL)
            .with_background(Color::Named(NamedColor::Blue))
            .with_font_style(FontStyle::BOLD)
            .with_border_all(Border::ROUNDED);

        assert_eq!(s.dimensions.width(), Some(Unit::FULL));
        assert_eq!(s.background, Some(Color::Named(NamedColor::Blue)));
        assert_eq!(s.font_style, Some(FontStyle::BOLD));
        assert_eq!(s.border, Some(Borders::all(Border::ROUNDED)));
        assert!(s.color.is_none());
    }

    #[test]
    fn has_layout_and_visuals() {
        let layout = Style::new().with_width(Unit::FULL);
        assert!(layout.has_layout());
        assert!(!layout.has_visuals());

        let visual = Style::new().with_color(Color::Named(NamedColor::Red));
        assert!(!visual.has_layout());
        assert!(visual.has_visuals());
    }

    // Ratatui integration
    #[test]
    #[cfg(feature = "ratatui")]
    fn convert_to_ratatui() { assert!(true) }
}
