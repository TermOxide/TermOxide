/// One style property: not mentioned, explicitly inherited, or set (ADR D2.2).
///
/// Serialized as `Option<Option<T>>` (`None` is `Unset`, `Some(None)` is
/// `Inherit`), so a schema trace sees the payload type of every property.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(from = "Option<Option<T>>", into = "Option<Option<T>>"))]
#[cfg_attr(
    feature = "serde",
    serde(bound(
        serialize = "T: serde::Serialize + Clone",
        deserialize = "T: serde::Deserialize<'de>"
    ))
)]
pub enum Property<T> {
    Unset,
    Inherit,
    Value(T),
}

impl<T: Copy> Property<T> {
    /// Last write wins; `Unset` keeps the left side (ADR D2.3).
    pub const fn merge(self, other: Self) -> Self {
        match other {
            Property::Unset => self,
            _ => other,
        }
    }

    /// The set value, if any. `Inherit` gives `None`: resolving it needs the
    /// parent.
    pub const fn value(self) -> Option<T> {
        match self {
            Property::Value(v) => Some(v),
            _ => None,
        }
    }
}

impl<T> From<Option<Option<T>>> for Property<T> {
    fn from(value: Option<Option<T>>) -> Self {
        match value {
            None => Property::Unset,
            Some(None) => Property::Inherit,
            Some(Some(v)) => Property::Value(v),
        }
    }
}

impl<T> From<Property<T>> for Option<Option<T>> {
    fn from(value: Property<T>) -> Self {
        match value {
            Property::Unset => None,
            Property::Inherit => Some(None),
            Property::Value(v) => Some(Some(v)),
        }
    }
}

/// What a property can change, which picks the dirty bit it sets (ADR D2.1,
/// D3.2). Paint properties are the ones that inherit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropClass {
    Paint,
    Layout,
    Both,
}

/// The work a style change causes, see [`super::Style::dirty`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Dirty {
    pub paint: bool,
    pub layout: bool,
}

impl Dirty {
    pub(crate) const fn mark(&mut self, class: PropClass) {
        match class {
            PropClass::Paint => self.paint = true,
            PropClass::Layout => self.layout = true,
            PropClass::Both => {
                self.paint = true;
                self.layout = true;
            },
        }
    }
}
