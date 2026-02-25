use std::borrow::Cow;
use std::hash::Hash;

/// A CSS-like string value.
///
/// Used for `font-family`, `content` (pseudo-elements), custom identifiers,
/// and any other property that takes a textual value.
///
/// # Zero-copy for static strings
///
/// The inner `Cow<'static, str>` means proc_macro-generated code like:
/// ```rust
/// let font = Str::from_static("JetBrains Mono");
/// ```
/// involves **zero heap allocation** — the slice lives in the binary's
/// read-only data segment. Runtime-computed strings fall back to
/// [`Str::from_string`] which heap-allocates via `Cow::Owned`.
///
/// Two `Str` values are equal if their **contents** are equal, regardless
/// of whether one is borrowed and the other owned.
///
/// # Examples
///
/// ```rust
/// let a: Str = "monospace".into();              // static borrow, no alloc
/// let b = Str::from_string(format!("Font-{}", 42)); // heap-allocated
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Str(pub Cow<'static, str>);

impl Str {
    /// Construct from a `'static` str — zero allocation.
    ///
    /// Preferred for proc_macro output.
    pub const fn from_static(s: &'static str) -> Self {
        Self(Cow::Borrowed(s))
    }

    /// Construct from a runtime-owned `String` — heap-allocates.
    pub fn from_string(s: String) -> Self {
        Self(Cow::Owned(s))
    }

    /// Borrow the inner string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns `true` if the string is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<&'static str> for Str {
    fn from(s: &'static str) -> Self {
        Self::from_static(s)
    }
}
impl From<String> for Str {
    fn from(s: String) -> Self {
        Self::from_string(s)
    }
}
impl AsRef<str> for Str {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for Str {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
