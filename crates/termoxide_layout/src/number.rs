use std::fmt::{Display, Formatter, Result};
use std::hash::{Hash, Hasher};
use std::ops::{Add, Mul, Neg, Sub};
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
/// use termoxide_layout::number::Int;
/// let z     = Int::new(10);
/// let order = Int::ZERO;
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Int(pub i32);

impl Int {
    /// Zero — the most common integer value in style systems.
    pub const ZERO: Self = Self(0);
    /// One — useful for `order: 1`, `column-count: 1`, etc.
    pub const ONE: Self = Self(1);

    /// Construct from a raw `i32`.
    pub const fn new(v: i32) -> Self {
        Self(v)
    }
    /// Extract the underlying `i32`.
    pub const fn get(self) -> i32 {
        self.0
    }
    /// Returns `true` if the value is zero.
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
    /// Returns `true` if the value is negative.
    pub const fn is_negative(self) -> bool {
        self.0 < 0
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
/// use termoxide_layout::number::Float;
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

// ---------------------------------------------------------------------------
//  Opacity
// ---------------------------------------------------------------------------

/// A self-validating opacity value bound to the CSS-legal interval
/// `[0.0, 1.0]`.
///
/// CSS `opacity` accepts `<number>` or `<percentage>`. The computed value
/// is always clamped to the unit interval; values outside that range have
/// no effect. We enforce the clamp at *construction time* so an `Opacity`
/// value is, by type, always within `[0.0, 1.0]`.
///
/// # Invariant
///
/// The inner [`Float`] is private and is guaranteed to satisfy
/// `0.0 <= v <= 1.0` and to be a finite number (never `NaN`, never
/// infinite). `f32::clamp` *propagates* NaN, so the constructor screens
/// for NaN first and maps it to `0.0` before clamping.
///
/// # Examples
///
/// ```rust
/// use termoxide_layout::number::Opacity;
///
/// assert_eq!(Opacity::new(1.5).get(), 1.0); // clamped down
/// assert_eq!(Opacity::new(-0.2).get(), 0.0); // clamped up
/// assert_eq!(Opacity::new(0.5).get(), 0.5); // pass-through
/// assert_eq!(Opacity::new(f32::NAN).get(), 0.0); // NaN → 0
///
/// // Const constants for the two endpoints.
/// assert_eq!(Opacity::OPAQUE.get(), 1.0);
/// assert_eq!(Opacity::TRANSPARENT.get(), 0.0);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Opacity(Float);

impl Opacity {
    /// Fully transparent (`opacity: 0`).
    pub const TRANSPARENT: Self = Self(Float::ZERO);
    /// Fully opaque (`opacity: 1`).
    pub const OPAQUE: Self = Self(Float::ONE);

    /// Construct from a raw `f32`, normalising into `[0.0, 1.0]`.
    ///
    /// `NaN` resolves to `0.0` (CSS treats invalid opacity as the initial
    /// value, but we choose the closer of the two endpoints), so the
    /// result is always a finite value inside the unit interval.
    pub fn new(v: f32) -> Self {
        let normalised = if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) };
        Self(Float::new(normalised))
    }

    /// Extract the inner `f32` (guaranteed to satisfy `0.0 <= v <= 1.0`).
    pub const fn get(self) -> f32 {
        self.0.0
    }

    /// Borrow the underlying [`Float`] (guaranteed clamped).
    pub const fn as_float(self) -> Float {
        self.0
    }
}

impl From<f32> for Opacity {
    fn from(v: f32) -> Self {
        Self::new(v)
    }
}

impl From<Float> for Opacity {
    fn from(v: Float) -> Self {
        Self::new(v.0)
    }
}
