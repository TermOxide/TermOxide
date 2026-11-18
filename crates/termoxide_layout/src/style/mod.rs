//! The style value: a typed property table merged by argument order, see the
//! style ADR, domains 2 and 5.

mod border;
mod color;
mod flex;
mod property;
mod text;
mod unit;

pub use border::{BorderSides, BorderStyle};
pub use color::{Color, Sem};
pub use flex::{Align, Display, FlexDirection, Justify, Position};
pub use property::{Dirty, PropClass, Property};
pub use text::{TextOverflow, TextWrap};
pub use unit::Unit;

macro_rules! normalized {
    ($value:expr) => {
        $value
    };
    ($value:expr, $norm:path) => {
        $norm($value)
    };
}

macro_rules! for_each_property {
    ($($(#[$meta:meta])* $field:ident / $id:ident: $ty:ty = $class:ident $(, $norm:path)?;)*) => {
        /// A partial style: every property is a [`Property`], `Unset` unless
        /// written (ADR D2.2).
        ///
        /// Setters clamp negative lengths and flex factors to zero.
        ///
        /// ```rust
        /// use termoxide_layout::style::{Color, Property, Sem, Style, Unit};
        ///
        /// const BUTTON: Style = Style::new().bg(Color::Semantic(Sem::Surface)).pad_x(2);
        /// const HOVER: Style = Style::new().bg(Color::Semantic(Sem::Accent));
        /// const BUTTON_HOVER: Style = BUTTON.merge(HOVER);
        ///
        /// assert_eq!(BUTTON_HOVER.bg, Property::Value(Color::Semantic(Sem::Accent)));
        /// assert_eq!(BUTTON_HOVER.padding_left, Property::Value(Unit::Cells(2)));
        /// ```
        #[derive(Clone, Copy, Debug, PartialEq)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        pub struct Style {
            $($(#[$meta])* pub $field: Property<$ty>,)*
        }

        /// Names a property of [`Style`], in declaration order.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum PropId {
            $($id,)*
        }

        /// The class of each property, indexed by `PropId as usize` (ADR D2.1).
        pub const CLASS: &[PropClass] = &[$(PropClass::$class,)*];

        impl PropId {
            pub const ALL: &[PropId] = &[$(PropId::$id,)*];

            pub const fn class(self) -> PropClass { CLASS[self as usize] }
        }

        impl Style {
            pub const fn new() -> Self { Self { $($field: Property::Unset,)* } }

            /// `other`'s set properties win; its `Unset` ones keep `self`'s
            /// (ADR D2.3).
            pub const fn merge(self, other: Self) -> Self {
                Self { $($field: self.$field.merge(other.$field),)* }
            }

            /// What changing from `self` to `new` invalidates (ADR D3.2).
            pub fn dirty(&self, new: &Self) -> Dirty {
                let mut dirty = Dirty::default();
                $(
                    if self.$field != new.$field {
                        dirty.mark(PropClass::$class);
                    }
                )*
                dirty
            }

            $(
                pub const fn $field(mut self, value: $ty) -> Self {
                    self.$field = Property::Value(normalized!(value $(, $norm)?));
                    self
                }
            )*
        }
    };
}

const fn non_negative(factor: f32) -> f32 { factor.max(0.0) }

for_each_property! {
    fg / Fg: Color = Paint;
    bg / Bg: Color = Paint;
    bold / Bold: bool = Paint;
    dim / Dim: bool = Paint;
    italic / Italic: bool = Paint;
    underline / Underline: bool = Paint;
    strikethrough / Strikethrough: bool = Paint;
    reverse / Reverse: bool = Paint;
    border_style / BorderStyle: BorderStyle = Paint;
    text_overflow / TextOverflow: TextOverflow = Paint;

    display / Display: Display = Layout;
    /// Border-box: includes padding and border (ADR D5.5).
    width / Width: Unit = Layout, Unit::non_negative;
    /// Border-box: includes padding and border (ADR D5.5).
    height / Height: Unit = Layout, Unit::non_negative;
    min_width / MinWidth: Unit = Layout, Unit::non_negative;
    max_width / MaxWidth: Unit = Layout, Unit::non_negative;
    min_height / MinHeight: Unit = Layout, Unit::non_negative;
    max_height / MaxHeight: Unit = Layout, Unit::non_negative;
    flex_basis / FlexBasis: Unit = Layout, Unit::non_negative;
    flex_direction / FlexDirection: FlexDirection = Layout;
    flex_grow / FlexGrow: f32 = Layout, non_negative;
    flex_shrink / FlexShrink: f32 = Layout, non_negative;
    align_items / AlignItems: Align = Layout;
    align_self / AlignSelf: Align = Layout;
    justify_content / JustifyContent: Justify = Layout;
    gap / Gap: Unit = Layout, Unit::non_negative;
    position / Position: Position = Layout;
    inset_top / InsetTop: Unit = Layout;
    inset_right / InsetRight: Unit = Layout;
    inset_bottom / InsetBottom: Unit = Layout;
    inset_left / InsetLeft: Unit = Layout;
    text_wrap / TextWrap: TextWrap = Layout;

    padding_top / PaddingTop: Unit = Both, Unit::non_negative;
    padding_right / PaddingRight: Unit = Both, Unit::non_negative;
    padding_bottom / PaddingBottom: Unit = Both, Unit::non_negative;
    padding_left / PaddingLeft: Unit = Both, Unit::non_negative;
    border_sides / BorderSides: BorderSides = Both;
}

impl Style {
    /// Sets the four paddings to `cells`.
    pub const fn padding(self, cells: i16) -> Self { self.pad_x(cells).pad_y(cells) }

    /// Sets the left and right paddings to `cells`.
    pub const fn pad_x(self, cells: i16) -> Self {
        self.padding_left(Unit::Cells(cells)).padding_right(Unit::Cells(cells))
    }

    /// Sets the top and bottom paddings to `cells`.
    pub const fn pad_y(self, cells: i16) -> Self {
        self.padding_top(Unit::Cells(cells)).padding_bottom(Unit::Cells(cells))
    }
}

impl Default for Style {
    fn default() -> Self { Self::new() }
}
