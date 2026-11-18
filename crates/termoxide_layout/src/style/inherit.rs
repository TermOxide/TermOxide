use std::rc::Rc;

use super::{BorderStyle, Color, Property, Style, TextOverflow};

macro_rules! define_inherited {
    ($($(#[$meta:meta])* $field:ident / $id:ident: $ty:ty = $class:ident $(, $norm:path)?;)*) => {
        define_inherited!(@paint [] $($field: $ty = $class;)*);

        fn inherit_layout(target: &mut Style, parent: Option<&Style>) {
            $(inherit_layout!($class, target, parent, $field);)*
        }
    };
    (@paint [$($acc:tt)*] $field:ident: $ty:ty = Paint; $($rest:tt)*) => {
        define_inherited!(@paint [$($acc)* $field: $ty;] $($rest)*);
    };
    (@paint [$($acc:tt)*] $field:ident: $ty:ty = $class:ident; $($rest:tt)*) => {
        define_inherited!(@paint [$($acc)*] $($rest)*);
    };
    (@paint [$($field:ident: $ty:ty;)*]) => {
        /// The resolved Paint properties of a node, shared by pointer with its
        /// parent when the node overrides none of them (ADR D4.2).
        ///
        /// `None` is the initial value: terminal default colour, attribute
        /// off.
        #[derive(Clone, Debug, Default, PartialEq)]
        pub struct Inherited {
            $(pub $field: Option<$ty>,)*
        }

        impl Inherited {
            fn overrides(own: &Style) -> bool {
                false $(|| matches!(own.$field, Property::Value(_)))*
            }

            fn overlay(&self, own: &Style) -> Self {
                Self { $($field: own.$field.value().or(self.$field),)* }
            }

            fn write_to(&self, target: &mut Style) {
                $(target.$field = self.$field.map_or(Property::Unset, Property::Value);)*
            }
        }
    };
}

macro_rules! inherit_layout {
    (Paint, $target:ident, $parent:ident, $field:ident) => {};
    ($class:ident, $target:ident, $parent:ident, $field:ident) => {
        if matches!($target.$field, Property::Inherit) {
            $target.$field = $parent.map_or(Property::Unset, |p| p.$field);
        }
    };
}

for_each_property!(define_inherited);

/// Resolves `own` against its parent's `target_style` and record, and returns
/// the node's `target_style` and record (ADR D4.1). A root passes `None`.
///
/// Paint properties left `Unset` or `Inherit` take the parent's value. Layout
/// properties take it only when `Inherit`, and stay `Unset` otherwise. The
/// returned style holds no `Inherit`.
///
/// ```rust
/// use std::rc::Rc;
///
/// use termoxide_layout::style::{Color, Property, Style, Unit, resolve};
///
/// let (panel, panel_rec) = resolve(&Style::new().fg(Color::Base16(2)).width(Unit::Cells(9)), None);
/// let (text, text_rec) = resolve(&Style::new(), Some((&panel, &panel_rec)));
///
/// assert!(Rc::ptr_eq(&panel_rec, &text_rec));
/// assert_eq!(text.fg, Property::Value(Color::Base16(2)));
/// assert_eq!(text.width, Property::Unset);
/// ```
pub fn resolve(own: &Style, parent: Option<(&Style, &Rc<Inherited>)>) -> (Style, Rc<Inherited>) {
    let inherited = match parent {
        Some((_, record)) if !Inherited::overrides(own) => Rc::clone(record),
        Some((_, record)) => Rc::new(record.overlay(own)),
        None => Rc::new(Inherited::default().overlay(own)),
    };
    let mut target = *own;
    inherited.write_to(&mut target);
    inherit_layout(&mut target, parent.map(|(style, _)| style));
    (target, inherited)
}
