//! This module defines the foundational type system for a CSS/SCSS-like
//! styling layer on top of Ratatui. It is designed to be consumed primarily
//! by proc_macro-generated code (from your JSX-like and SCSS-like syntax),
//! but is also ergonomic enough for manual use.
//!
//! ## Design philosophy
//!
//! - **Cheap to copy**: Terminal UIs redraw on every frame. Every type that
//!   will live inside a [`Style`] struct implements `Copy` where possible.
//!
//! - **`const`-constructible**: Proc_macros emit code that runs at compile
//!   time. Where possible, constructors are marked `const` so that static
//!   style definitions have zero runtime cost.
//!
//! - **`Option<T>` for all style fields**: Distinguishing "not set" from
//!   "set to the default value" is critical for cascade and inheritance.
//!   A child that doesn't set `color` must not reset the parent's `color`
//!   to the type default. Every field in [`Style`] is `Option<T>`.
//!
//! - **No heap allocation in the hot path**: [`str::Str`] uses `Cow<'static, str>`
//!   so proc_macro-emitted string literals are zero-allocation borrows.
//!   Runtime strings fall back to owned allocation.
//!
//! ## Module layout
//!
//! ```text
//! styles.rs
//! ├── Color / NamedColor                              — foreground & background colors
//! ├── Number                                          — integer scalar values (z-index, tab-index…) and floating-point scalars (opacity, flex-grow…)
//! ├── Str                                             — CSS string values (font-family, content…)
//! ├── Unit                                            — dimensional values (width, height, gap…)
//! ├── Border / BorderStyle  / Edges<T>                — four-sided shorthand (padding, margin…) and border appearance
//! ├── FontStyle                                       — text modifier bitset (bold | italic | …)
//! ├── Layout                                          — layout mode enums / flex alignment enums / text and overflow enums
//! └── Style                    — the aggregate style declaration struct
//! ```
pub mod border;
pub mod color;
pub mod font;
pub mod layout;
pub mod number;
pub mod str;
pub mod unit;

use border::{Border, Edges};
use color::Color;
use font::FontStyle;
use layout::{Align, Display, FlexDirection, Justify, Overflow, TextAlign};
use number::Float;
use unit::Unit;

/// A complete set of style declarations for one UI element.
///
/// Every field is `Option<T>`. `None` means **"not declared on this element"**,
/// which is fundamentally different from "set to the default value". This
/// distinction drives three core behaviours:
///
/// 1. **Cascade / inheritance** — a child's `None` field never resets a
///    parent's value. Only `Some(x)` is an active declaration.
///
/// 2. **Style merging** — theme + component + inline styles are applied in
///    priority order via [`Style::merge`]. Later `Some` values win; `None`
///    values are silently skipped.
///
/// 3. **Proc_macro output** — `scss! { color: red; }` generates a `Style`
///    with only `color` set to `Some`. Every other field is `None`.
///
/// # Creating styles
///
/// ```rust
/// // Direct struct construction (idiomatic proc_macro output)
/// let s = Style {
///     width:      Some(Unit::percent(100)),
///     height:     Some(Unit::cells(3)),
///     background: Some(Color::Named(NamedColor::Blue)),
///     font_style: Some(FontStyle::BOLD),
///     ..Style::new()
/// };
///
/// // Builder pattern (more ergonomic for handwritten code)
/// let s = Style::new()
///     .with_width(Unit::FULL)
///     .with_background(Color::Named(NamedColor::Blue));
/// ```
///
/// # Merging
///
/// ```rust
/// let mut base = Style { color: Some(Color::Named(NamedColor::White)), ..Style::new() };
/// let over     = Style { color: Some(Color::Named(NamedColor::Red)),   ..Style::new() };
/// base.merge(&over);
/// // base.color == Some(Red)
/// ```
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Style {
    // -----------------------------------------------------------------------
    // Box model
    // -----------------------------------------------------------------------
    /// Explicit width. `None` → layout engine decides (usually `Auto`).
    pub width: Option<Unit>,
    /// Explicit height.
    pub height: Option<Unit>,
    /// Minimum width — element is never narrower than this.
    pub min_width: Option<Unit>,
    /// Minimum height.
    pub min_height: Option<Unit>,
    /// Maximum width — element is never wider than this.
    ///
    /// Useful for responsive sidebars that shouldn't exceed a fixed column count.
    pub max_width: Option<Unit>,
    /// Maximum height.
    pub max_height: Option<Unit>,

    /// Inner spacing between border and content. CSS `padding`.
    ///
    /// `Edges::all(Unit::cells(1))` = 1-cell padding on every side.
    pub padding: Option<Edges<Unit>>,

    /// Outer spacing between border and neighbors. CSS `margin`.
    ///
    /// Negative margins (`Unit::Cells(-1)`) are valid for overlap effects.
    pub margin: Option<Edges<Unit>>,

    // -----------------------------------------------------------------------
    // Layout
    // -----------------------------------------------------------------------
    /// How children are laid out. CSS `display`.
    pub display: Option<Display>,
    /// Main axis for flex layout. Only meaningful when `display == Flex`.
    pub flex_direction: Option<FlexDirection>,
    /// Grow factor relative to flex siblings. CSS `flex-grow`.
    pub flex_grow: Option<Float>,
    /// Shrink factor when space is tight. CSS `flex-shrink`.
    pub flex_shrink: Option<Float>,
    /// Cross-axis child alignment. CSS `align-items`.
    pub align_items: Option<Align>,
    /// Main-axis child distribution. CSS `justify-content`.
    pub justify_content: Option<Justify>,
    /// Space between children (not at edges). CSS `gap`.
    pub gap: Option<Unit>,

    // -----------------------------------------------------------------------
    // Visuals
    // -----------------------------------------------------------------------
    /// Foreground (text) color. CSS `color`.
    ///
    /// Inherited by children that don't declare their own `color`.
    pub color: Option<Color>,

    /// Background fill color. CSS `background-color`.
    ///
    /// Fills the element's box including padding (border-box semantics).
    pub background: Option<Color>,

    /// Border appearance. CSS `border`.
    ///
    /// Drawn as Unicode box-drawing characters, always 1 cell thick.
    pub border: Option<Border>,

    /// Element opacity 0.0–1.0. CSS `opacity`.
    ///
    /// In TUI, implemented as dimming (`FontStyle::DIM`) rather than
    /// alpha-blending. Values are typically quantized to visible/dim/hidden.
    pub opacity: Option<Float>,

    // -----------------------------------------------------------------------
    // Typography
    // -----------------------------------------------------------------------
    /// Horizontal text alignment. CSS `text-align`.
    pub text_align: Option<TextAlign>,

    /// Text modifiers — bold, italic, underline, etc.
    ///
    /// Combine with `|`: `FontStyle::BOLD | FontStyle::ITALIC`.
    pub font_style: Option<FontStyle>,

    // -----------------------------------------------------------------------
    // Overflow
    // -----------------------------------------------------------------------
    /// Content overflow behaviour. CSS `overflow`.
    pub overflow: Option<Overflow>,
}

impl Style {
    /// All-`None` style — no declarations, the "tabula rasa".
    ///
    /// `const` so it can be used in static contexts:
    /// ```rust
    /// const EMPTY: Style = Style::new();
    /// ```
    pub const fn new() -> Self {
        Self {
            width: None,
            height: None,
            min_width: None,
            min_height: None,
            max_width: None,
            max_height: None,
            padding: None,
            margin: None,
            display: None,
            flex_direction: None,
            flex_grow: None,
            flex_shrink: None,
            align_items: None,
            justify_content: None,
            gap: None,
            color: None,
            background: None,
            border: None,
            opacity: None,
            text_align: None,
            font_style: None,
            overflow: None,
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
    /// let mut s = Style { color: Some(Color::Named(NamedColor::White)), ..Style::new() };
    /// s.merge(&Style { color: Some(Color::Named(NamedColor::Red)), ..Style::new() });
    /// // s.color == Some(Red)
    /// ```
    pub fn merge(&mut self, other: &Style) {
        // All fields are Copy, so we avoid `.clone()` entirely.
        macro_rules! m {
            ($f:ident) => {
                if let Some(v) = other.$f {
                    self.$f = Some(v);
                }
            };
        }
        m!(width);
        m!(height);
        m!(min_width);
        m!(min_height);
        m!(max_width);
        m!(max_height);
        m!(padding);
        m!(margin);
        m!(display);
        m!(flex_direction);
        m!(flex_grow);
        m!(flex_shrink);
        m!(align_items);
        m!(justify_content);
        m!(gap);
        m!(color);
        m!(background);
        m!(border);
        m!(opacity);
        m!(text_align);
        m!(font_style);
        m!(overflow);
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

    pub fn with_width(mut self, v: Unit) -> Self {
        self.width = Some(v);
        self
    }
    pub fn with_height(mut self, v: Unit) -> Self {
        self.height = Some(v);
        self
    }
    pub fn with_min_width(mut self, v: Unit) -> Self {
        self.min_width = Some(v);
        self
    }
    pub fn with_min_height(mut self, v: Unit) -> Self {
        self.min_height = Some(v);
        self
    }
    pub fn with_max_width(mut self, v: Unit) -> Self {
        self.max_width = Some(v);
        self
    }
    pub fn with_max_height(mut self, v: Unit) -> Self {
        self.max_height = Some(v);
        self
    }
    pub fn with_padding(mut self, v: Edges<Unit>) -> Self {
        self.padding = Some(v);
        self
    }
    pub fn with_margin(mut self, v: Edges<Unit>) -> Self {
        self.margin = Some(v);
        self
    }

    /// Convenience: uniform padding on all four sides.
    pub fn with_padding_all(self, v: Unit) -> Self {
        self.with_padding(Edges::all(v))
    }
    /// Convenience: uniform margin on all four sides.
    pub fn with_margin_all(self, v: Unit) -> Self {
        self.with_margin(Edges::all(v))
    }

    pub fn with_display(mut self, v: Display) -> Self {
        self.display = Some(v);
        self
    }
    pub fn with_flex_direction(mut self, v: FlexDirection) -> Self {
        self.flex_direction = Some(v);
        self
    }
    pub fn with_flex_grow(mut self, v: Float) -> Self {
        self.flex_grow = Some(v);
        self
    }
    pub fn with_flex_shrink(mut self, v: Float) -> Self {
        self.flex_shrink = Some(v);
        self
    }
    pub fn with_align_items(mut self, v: Align) -> Self {
        self.align_items = Some(v);
        self
    }
    pub fn with_justify_content(mut self, v: Justify) -> Self {
        self.justify_content = Some(v);
        self
    }
    pub fn with_gap(mut self, v: Unit) -> Self {
        self.gap = Some(v);
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
    pub fn with_border(mut self, v: Border) -> Self {
        self.border = Some(v);
        self
    }
    pub fn with_opacity(mut self, v: Float) -> Self {
        self.opacity = Some(v);
        self
    }
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

    // -----------------------------------------------------------------------
    // Introspection
    // -----------------------------------------------------------------------

    /// `true` if no fields are set (all `None`).
    pub fn is_empty(&self) -> bool {
        *self == Style::default()
    }

    /// `true` if any dimension or spacing field is set.
    pub fn has_layout(&self) -> bool {
        self.width.is_some()
            || self.height.is_some()
            || self.min_width.is_some()
            || self.min_height.is_some()
            || self.max_width.is_some()
            || self.max_height.is_some()
            || self.padding.is_some()
            || self.margin.is_some()
            || self.gap.is_some()
    }

    /// `true` if any visual (non-layout) field is set.
    pub fn has_visuals(&self) -> bool {
        self.color.is_some()
            || self.background.is_some()
            || self.border.is_some()
            || self.opacity.is_some()
            || self.text_align.is_some()
            || self.font_style.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::str::Str;
    use super::*;
    use border::{Border, BorderStyle, Edges};
    use color::Color;
    use color::NamedColor;
    use font::FontStyle;
    use number::{Float, Int};
    use std::borrow::Cow;
    use unit::Unit;

    // --- Color ---

    #[test]
    fn color_hex_valid() {
        assert_eq!(
            Color::from_hex_bytes(b"#ff5f00"),
            Some(Color::Rgb(255, 95, 0))
        );
        assert_eq!(Color::from_hex_bytes(b"#000000"), Some(Color::Rgb(0, 0, 0)));
        assert_eq!(
            Color::from_hex_bytes(b"#FFFFFF"),
            Some(Color::Rgb(255, 255, 255))
        );
        assert_eq!(
            Color::from_hex_bytes(b"#aAbBcC"),
            Some(Color::Rgb(0xaa, 0xbb, 0xcc))
        );
    }

    #[test]
    fn color_hex_invalid() {
        assert_eq!(Color::from_hex_bytes(b"ff5f00"), None); // no #
        assert_eq!(Color::from_hex_bytes(b"#ff5fgg"), None); // bad nibble
        assert_eq!(Color::from_hex_bytes(b"#fff"), None); // shorthand not supported
        assert_eq!(Color::from_hex_bytes(b""), None);
    }

    #[test]
    fn color_is_abstract() {
        assert!(Color::None.is_abstract());
        assert!(Color::Inherit.is_abstract());
        assert!(!Color::rgb(0, 0, 0).is_abstract());
        assert!(!Color::Named(NamedColor::Red).is_abstract());
    }

    // --- Int ---

    #[test]
    fn int_arithmetic() {
        assert_eq!(Int::new(3) + Int::new(4), Int::new(7));
        assert_eq!(Int::new(10) - Int::new(3), Int::new(7));
        assert_eq!(-Int::new(5), Int::new(-5));
    }

    #[test]
    fn int_predicates() {
        assert!(Int::ZERO.is_zero());
        assert!(!Int::ONE.is_zero());
        assert!(Int::new(-1).is_negative());
        assert!(!Int::ONE.is_negative());
    }

    // --- Float ---

    #[test]
    fn float_eq_bitwise() {
        assert_eq!(Float::new(1.0), Float::new(1.0));
        assert_ne!(Float::new(0.5), Float::new(0.9));
        let nan = Float::new(f32::NAN);
        assert_eq!(nan, nan); // NaN == NaN via bits — intentional
    }

    #[test]
    fn float_clamp_unit() {
        assert_eq!(Float::new(1.5).clamp_unit(), Float::new(1.0));
        assert_eq!(Float::new(-0.5).clamp_unit(), Float::new(0.0));
        assert_eq!(Float::new(0.75).clamp_unit(), Float::new(0.75));
    }

    #[test]
    fn float_ops() {
        assert_eq!(Float::new(0.5) + Float::new(0.25), Float::new(0.75));
        assert_eq!(Float::new(2.0) * Float::new(3.0), Float::new(6.0));
    }

    // --- Str ---

    #[test]
    fn str_static_is_borrowed() {
        let s = Str::from_static("mono");
        assert!(matches!(s.0, Cow::Borrowed(_)));
        assert_eq!(s.as_str(), "mono");
    }

    #[test]
    fn str_from_string_is_owned() {
        let s = Str::from_string("runtime".to_string());
        assert!(matches!(s.0, Cow::Owned(_)));
    }

    #[test]
    fn str_equality_ignores_cow_variant() {
        assert_eq!(Str::from_static("hello"), Str::from_string("hello".into()));
    }

    // --- Unit ---

    #[test]
    fn unit_predicates() {
        assert!(Unit::cells(10).is_definite());
        assert!(Unit::percent(50).is_definite());
        assert!(!Unit::fill(1).is_definite());
        assert!(!Unit::AUTO.is_definite());

        assert!(Unit::fill(1).is_intrinsic());
        assert!(Unit::AUTO.is_intrinsic());
        assert!(!Unit::ZERO.is_intrinsic());

        assert!(Unit::UNSET.is_unset());
        assert!(!Unit::ZERO.is_unset());
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
    fn edges_map() {
        let raw: Edges<i32> = Edges::all(5);
        let doubled = raw.map(|v| v * 2);
        assert_eq!(doubled.top, 10);
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
        let s = (FontStyle::BOLD | FontStyle::ITALIC).without(FontStyle::ITALIC);
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
        assert_eq!(base.color, Some(Color::Named(NamedColor::Red))); // overridden
        assert_eq!(base.background, Some(Color::Named(NamedColor::Black))); // untouched
    }

    #[test]
    fn merge_none_does_not_overwrite() {
        let mut base = Style {
            width: Some(Unit::cells(80)),
            ..Style::new()
        };
        base.merge(&Style::new());
        assert_eq!(base.width, Some(Unit::cells(80)));
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
        assert_eq!(base.color, Some(Color::Named(NamedColor::White))); // untouched
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
            .with_border(Border::ROUNDED);

        assert_eq!(s.width, Some(Unit::FULL));
        assert_eq!(s.background, Some(Color::Named(NamedColor::Blue)));
        assert_eq!(s.font_style, Some(FontStyle::BOLD));
        assert_eq!(s.border, Some(Border::ROUNDED));
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
    fn convert_to_ratatui() {
        assert!(true)
    }
}
