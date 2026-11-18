/// Where text breaks onto a new line. `Word` is approximate for scripts
/// without spaces, such as CJK.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TextWrap {
    None,
    Char,
    Word,
}

/// What text does at the end of its node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TextOverflow {
    Clip,
    Ellipsis,
}
