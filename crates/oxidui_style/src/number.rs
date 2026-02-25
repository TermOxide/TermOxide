use std::fmt::{self, Display, Formatter, Result};
use std::hash::{Hash, Hasher};
use std::ops::{Add, Mul, Neg, Sub};
use std::str::FromStr;
/// A CSS-like integer scalar value.
///
/// Used for whole-number properties: `z-index`, `tab-index`,
/// `column-count`, `order` (flex item ordering), etc.
///
/// Kept as a newtype rather than bare `i32` so that:
/// 1. Conversion traits can be implemented without orphan-rule conflicts.
/// 2. Functions taking `Int` are self-documenting — it's clearly a CSS
///    integer, not an arbitrary `i32`.
/// 3. The proc_macro can distinguish `Int` from [`super::unit::Unit`] and [`Float`]
///    at the type level, preventing category errors.
///
/// # Examples
///
/// ```rust
/// let z     = Int::new(10);
/// let order = Int::ZERO;
/// ```

/// A CSS-like integer scalar value.
///
/// Supports:
/// - Binary literals: `0b1010`
/// - Octal literals: `0o12`
/// - Hex literals: `0xFF`
/// - Decimal literals: `42`
///
/// Internally always stored as `i32`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Int(pub i32);

#[derive(Debug)]
pub enum IntParseError {
    InvalidRadix,
    ParseIntError(std::num::ParseIntError),
}

impl Display for IntParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRadix => write!(f, "invalid radix (must be 2–36)"),
            Self::ParseIntError(e) => write!(f, "parse int error: {:?}", e.kind()),
        }
    }
}
impl Int {
    /// Zero constant.
    pub const ZERO: Self = Self(0);

    /// One constant.
    pub const ONE: Self = Self(1);

    /// Construct from raw `i32`.
    pub const fn new(v: i32) -> Self {
        Self(v)
    }

    /// Extract inner value.
    pub const fn get(self) -> i32 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    pub const fn is_negative(self) -> bool {
        self.0 < 0
    }

    /// Precise radix constructor (2–36).
    pub fn from_str_radix(s: &str, radix: u32) -> std::result::Result<Self, IntParseError> {
        if (radix < 2) || (radix > 36) {
            return Err(IntParseError::InvalidRadix);
        }
        i32::from_str_radix(s, radix)
            .map(Self)
            .map_err(IntParseError::ParseIntError)
    }

    /// Format as binary with `0b` prefix.
    pub fn to_bin(self) -> String {
        format!("0b{:b}", self.0)
    }

    /// Format as octal with `0o` prefix.
    pub fn to_oct(self) -> String {
        format!("0o{:o}", self.0)
    }

    /// Format as lowercase hex with `0x` prefix.
    pub fn to_hex(self) -> String {
        format!("0x{:x}", self.0)
    }

    /// Format as uppercase hex with `0X` prefix.
    pub fn to_hex_upper(self) -> String {
        format!("0X{:X}", self.0)
    }
}

impl FromStr for Int {
    type Err = std::num::ParseIntError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let s = s.trim();

        // Handle optional sign
        let (negative, body) = if let Some(rest) = s.strip_prefix('-') {
            (true, rest)
        } else if let Some(rest) = s.strip_prefix('+') {
            (false, rest)
        } else {
            (false, s)
        };

        let parsed = if let Some(rest) = body.strip_prefix("0b") {
            i32::from_str_radix(rest, 2)?
        } else if let Some(rest) = body.strip_prefix("0o") {
            i32::from_str_radix(rest, 8)?
        } else if let Some(rest) = body.strip_prefix("0x") {
            i32::from_str_radix(rest, 16)?
        } else {
            body.parse::<i32>()?
        };

        Ok(Self(if negative { -parsed } else { parsed }))
    }
}

impl From<i32> for Int {
    fn from(v: i32) -> Self {
        Self(v)
    }
}

impl From<Int> for i32 {
    fn from(v: Int) -> Self {
        v.0
    }
}

impl Display for Int {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{}", self.0)
    }
}

impl Add for Int {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl Sub for Int {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0)
    }
}

impl Neg for Int {
    type Output = Self;
    fn neg(self) -> Self {
        Self(-self.0)
    }
}

/// A CSS-like floating-point scalar value.
///
/// Used for `opacity`, `flex-grow`, `flex-shrink`, `aspect-ratio` — any
/// real-number property that isn't a spatial dimension (use [`super::unit::Unit`] for those).
///
/// # Why not bare `f32`?
///
/// `f32` doesn't implement `Hash` or `Eq`, making it impossible to use
/// style structs as `HashMap` keys or in `HashSet`s. We implement both
/// traits via **bitwise comparison** of the IEEE 754 bit pattern:
///
/// - Equal floats have equal bits → hashes are consistent. ✓
/// - `NaN == NaN` under bit equality (same payload). This is intentional:
///   NaN is not a valid style value and should never appear in production.
///   If the proc_macro validates inputs at parse time, NaN never reaches
///   runtime.
///
/// # Examples
///
/// ```rust
/// let opacity = Float::new(0.85);
/// let grow    = Float::ONE;
/// let shrink  = Float::ZERO;
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct Float(pub f32);

impl Float {
    /// `0.0` — default for `opacity: 0`, `flex-grow: 0`, etc.
    pub const ZERO: Self = Self(0.0);
    /// `1.0` — default for `opacity: 1`, `flex-grow: 1`, etc.
    pub const ONE: Self = Self(1.0);
    /// `0.5` — a convenient halfway value.
    pub const HALF: Self = Self(0.5);

    /// Construct from a raw `f32`.
    pub const fn new(v: f32) -> Self {
        Self(v)
    }
    /// Extract the underlying `f32`.
    pub const fn get(self) -> f32 {
        self.0
    }

    /// Clamp to `[0.0, 1.0]`.
    ///
    /// Useful for `opacity`, `flex-shrink`, or any unit-interval property.
    pub fn clamp_unit(self) -> Self {
        Self(self.0.clamp(0.0, 1.0))
    }

    /// Returns `true` if the value is exactly `0.0`.
    pub fn is_zero(self) -> bool {
        self.0 == 0.0
    }
}

/// Bit-equality. See type-level docs for NaN rationale.
impl PartialEq for Float {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

/// Derived from bit-equality — see [`PartialEq`] impl.
impl Eq for Float {}

/// Bit-pattern hash — consistent with the `PartialEq` implementation.
impl Hash for Float {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.to_bits().hash(state);
    }
}

impl PartialOrd for Float {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

impl From<f32> for Float {
    fn from(v: f32) -> Self {
        Self(v)
    }
}
impl From<Float> for f32 {
    fn from(v: Float) -> Self {
        v.0
    }
}

impl Display for Float {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{}", self.0)
    }
}

impl Add for Float {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}
impl Mul for Float {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        Self(self.0 * rhs.0)
    }
}
