// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `long double` as a soft value type over the target ABI's exact bit pattern.
//!
//! Stable Rust has no `f80` and `f128` is nightly, so this module carries the value itself:
//!
//! * x86-64: x87 80-bit extended in a 16-byte slot — 1 sign bit, 15 exponent bits, a 64-bit
//!   significand with the **explicit** integer bit at bit 63, and six bytes of tail padding
//!   (NetBSD/AMD64 `long double` layout, donor-map 05 39-44);
//! * aarch64 and riscv64: IEEE binary128 in the same 16-byte slot — 1 sign bit, 15 exponent bits,
//!   a 113-bit significand with the hidden integer bit.
//!
//! `size_of` and `align_of` are 16 on all three targets; the value is never an `f64` alias.
//!
//! The x86-64 arithmetic engine is written from the IEEE 754 rules (round to nearest even,
//! explicit-integer-bit normalisation, pseudo-denormals treated as subnormals). The binary128
//! targets have the representation, classification, comparison and conversion paths; their
//! arithmetic is not implemented yet and is listed in the C47c report.

use core::cmp::Ordering;

use crate::consts::{FP_INFINITE, FP_NAN, FP_NORMAL, FP_SUBNORMAL, FP_ZERO};

/// `long double` value: 16 bytes, 16-byte aligned on every supported target.
#[repr(C, align(16))]
#[derive(Clone, Copy, Default)]
pub struct LongDouble {
    repr: u128,
}

const SIGN_BIT: u32 = 79;
const SIGN_MASK: u128 = 1u128 << SIGN_BIT;
const EXP_MASK: u32 = 0x7fff;
const EXP_BIAS: i32 = 16383;
const INT_BIT: u64 = 1 << 63;
const QNAN: u128 = ((EXP_MASK as u128) << 64) | (INT_BIT as u128) | (1 << 62);

const _: () = {
    assert!(core::mem::size_of::<LongDouble>() == 16);
    assert!(core::mem::align_of::<LongDouble>() == 16);
};

impl LongDouble {
    /// Build from the raw 16-byte pattern (little-endian; the x87 slot keeps the 80-bit value in
    /// the low ten bytes, matching the AMD64 ABI's six padding bytes).
    pub const fn from_bits(repr: u128) -> Self {
        Self { repr }
    }

    /// The raw pattern.
    pub const fn to_bits(self) -> u128 {
        self.repr
    }

    /// Signed zero.
    pub const fn zero(negative: bool) -> Self {
        Self {
            repr: if negative { SIGN_MASK } else { 0 },
        }
    }

    /// One.
    pub const fn one() -> Self {
        Self::from_parts(false, EXP_BIAS as u32, INT_BIT)
    }

    /// A quiet NaN.
    pub const fn nan() -> Self {
        Self { repr: QNAN }
    }

    /// An infinity.
    pub const fn infinity(negative: bool) -> Self {
        let sign = if negative { SIGN_MASK } else { 0 };
        Self {
            repr: sign | ((EXP_MASK as u128) << 64) | (INT_BIT as u128),
        }
    }

    /// Build from an explicit sign, biased exponent and 64-bit significand.
    pub const fn from_parts(negative: bool, exponent: u32, significand: u64) -> Self {
        let sign = if negative { SIGN_MASK } else { 0 };
        Self {
            repr: sign | (((exponent & EXP_MASK) as u128) << 64) | (significand as u128),
        }
    }

    /// The sign bit.
    pub const fn sign_bit(self) -> bool {
        self.repr & SIGN_MASK != 0
    }

    /// The biased exponent field.
    pub const fn exponent_field(self) -> u32 {
        ((self.repr >> 64) & EXP_MASK as u128) as u32
    }

    /// The 64-bit significand bits (with the explicit integer bit).
    pub const fn significand_bits(self) -> u64 {
        self.repr as u64
    }

    /// The C `fpclassify` class.
    pub fn class(self) -> i32 {
        let exponent = self.exponent_field();
        let significand = self.significand_bits();
        if exponent == EXP_MASK {
            if significand & !INT_BIT != 0 {
                FP_NAN
            } else {
                FP_INFINITE
            }
        } else if exponent == 0 {
            if significand == 0 {
                FP_ZERO
            } else {
                // A pseudo-denormal (integer bit set with a zero exponent field) is a valid
                // subnormal value here, never a normal.
                FP_SUBNORMAL
            }
        } else if significand & INT_BIT == 0 {
            // An invalid encoding (zero integer bit with a non-zero exponent): the x87 would
            // fault; the soft type reports it as subnormal instead of inventing a value.
            FP_SUBNORMAL
        } else {
            FP_NORMAL
        }
    }

    /// Whether the value is a NaN.
    pub fn is_nan(self) -> bool {
        self.class() == FP_NAN
    }

    /// Whether the value is infinite.
    pub fn is_infinite(self) -> bool {
        self.class() == FP_INFINITE
    }

    /// Whether the value is finite (including zero and subnormals).
    pub fn is_finite(self) -> bool {
        matches!(self.class(), FP_ZERO | FP_SUBNORMAL | FP_NORMAL)
    }

    /// Whether the value is zero.
    pub fn is_zero(self) -> bool {
        self.class() == FP_ZERO
    }

    /// Whether the value is a normal number.
    pub fn is_normal(self) -> bool {
        self.class() == FP_NORMAL
    }

    /// `|x|`.
    pub fn abs(self) -> Self {
        Self {
            repr: self.repr & !SIGN_MASK,
        }
    }

    /// `x` with the sign of `sign`.
    pub fn copysign(self, sign: Self) -> Self {
        Self {
            repr: (self.repr & !SIGN_MASK) | (sign.repr & SIGN_MASK),
        }
    }

    /// The value as an `f64`, correctly rounded (round to nearest even).
    pub fn to_f64(self) -> f64 {
        if self.is_nan() {
            return f64::NAN;
        }
        if self.is_infinite() {
            return if self.sign_bit() {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            };
        }
        if self.is_zero() {
            return if self.sign_bit() { -0.0 } else { 0.0 };
        }
        let negative = self.sign_bit();
        let field = self.exponent_field() as i32;
        let mut significand = self.significand_bits() as u128;
        // value = significand * 2^(exponent - 63)
        let mut exponent = if field == 0 {
            1 - EXP_BIAS
        } else {
            field - EXP_BIAS
        };
        while significand & (1 << 63) == 0 {
            significand <<= 1;
            exponent -= 1;
        }
        // Keep 53 bits and round.
        let drop = 64 - 53;
        let kept = (significand >> drop) as u64;
        let lost = significand & ((1u128 << drop) - 1);
        let half = 1u128 << (drop - 1);
        let mut mantissa = kept;
        if lost > half || (lost == half && (mantissa & 1) != 0) {
            mantissa += 1;
            if mantissa == 1 << 53 {
                mantissa >>= 1;
                exponent += 1;
            }
        }
        let sign = if negative { 1u64 << 63 } else { 0 };
        if exponent > 1023 {
            return if negative {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            };
        }
        if exponent < -1022 {
            let shift = -1022 - exponent;
            if shift > 53 {
                return if negative { -0.0 } else { 0.0 };
            }
            let mut sub = mantissa >> shift;
            let lost = mantissa & ((1u64 << shift) - 1);
            let half = 1u64 << (shift - 1);
            if lost > half || (lost == half && (sub & 1) != 0) {
                sub += 1;
            }
            return f64::from_bits(sign | sub);
        }
        f64::from_bits(sign | (((exponent + 1023) as u64) << 52) | (mantissa & ((1 << 52) - 1)))
    }

    /// The value as an `f32`, correctly rounded.
    pub fn to_f32(self) -> f32 {
        self.to_f64() as f32
    }

    /// An `f64` as a `long double` (exact for every value).
    pub fn from_f64(value: f64) -> Self {
        let bits = value.to_bits();
        let negative = bits >> 63 != 0;
        let field = ((bits >> 52) & 0x7ff) as i32;
        let fraction = bits & ((1u64 << 52) - 1);
        if field == 0x7ff {
            return if fraction != 0 {
                Self::nan()
            } else {
                Self::infinity(negative)
            };
        }
        if field == 0 && fraction == 0 {
            return Self::zero(negative);
        }
        // value = mantissa * 2^exponent, mantissa 53 bits for normals.
        let (mantissa, exponent) = if field == 0 {
            (fraction, -1074)
        } else {
            (fraction | (1u64 << 52), field - 1023 - 52)
        };
        let shift = mantissa.leading_zeros();
        let significand = (mantissa as u128) << shift;
        pack_exact(negative, exponent - shift as i32, significand as u64)
    }

    /// An `f32` as a `long double`.
    pub fn from_f32(value: f32) -> Self {
        Self::from_f64(value as f64)
    }

    /// A signed integer, exactly.
    pub fn from_i64(value: i64) -> Self {
        Self::from_u64_parts(value < 0, value.unsigned_abs())
    }

    /// An unsigned integer, exactly.
    pub fn from_u64(value: u64) -> Self {
        Self::from_u64_parts(false, value)
    }

    fn from_u64_parts(negative: bool, magnitude: u64) -> Self {
        if magnitude == 0 {
            return Self::zero(negative);
        }
        let shift = magnitude.leading_zeros();
        let significand = ((magnitude as u128) << shift) as u64;
        pack_exact(negative, -(shift as i32), significand)
    }

    /// Truncate toward zero as `i64`; `None` when the value does not fit or is not finite.
    pub fn to_i64(self) -> Option<i64> {
        let (negative, magnitude) = self.integer_magnitude()?;
        let value = i64::try_from(magnitude).ok()?;
        Some(if negative { -value } else { value })
    }

    /// Truncate toward zero as `u64`; `None` when negative, non-finite or too large.
    pub fn to_u64(self) -> Option<u64> {
        let (negative, magnitude) = self.integer_magnitude()?;
        if negative {
            return None;
        }
        u64::try_from(magnitude).ok()
    }

    fn integer_magnitude(self) -> Option<(bool, u128)> {
        if !self.is_finite() {
            return None;
        }
        if self.is_zero() || self.exponent_field() == 0 {
            return Some((self.sign_bit(), 0));
        }
        let negative = self.sign_bit();
        let exponent = self.exponent_field() as i32 - EXP_BIAS;
        let significand = self.significand_bits() as u128;
        let shift = exponent - 63;
        let magnitude = if shift >= 0 {
            if shift > 127 {
                return None;
            }
            significand << shift
        } else if -shift >= 128 {
            0
        } else {
            significand >> (-shift) as u32
        };
        Some((negative, magnitude))
    }

    /// A total order (C23 `totalorder`): `-NaN` below `-inf`, `-0` below `+0`, `+NaN` above `+inf`.
    pub fn total_cmp(self, other: Self) -> Ordering {
        order_key(self).cmp(&order_key(other))
    }

    /// The IEEE partial order; `None` when either operand is a NaN.
    pub fn partial_cmp(self, other: Self) -> Option<Ordering> {
        if self.is_nan() || other.is_nan() {
            return None;
        }
        Some(self.total_cmp(other))
    }

    /// `x + y`.
    #[allow(clippy::should_implement_trait)] // a soft value type: the operators are the C API
    #[cfg(target_arch = "x86_64")]
    pub fn add(self, other: Self) -> Self {
        arithmetic::add(self, other, false)
    }

    /// `x - y`.
    #[allow(clippy::should_implement_trait)]
    #[cfg(target_arch = "x86_64")]
    pub fn sub(self, other: Self) -> Self {
        arithmetic::add(self, other, true)
    }

    /// `x * y`.
    #[allow(clippy::should_implement_trait)]
    #[cfg(target_arch = "x86_64")]
    pub fn mul(self, other: Self) -> Self {
        arithmetic::mul(self, other)
    }

    /// `x / y`.
    #[allow(clippy::should_implement_trait)]
    #[cfg(target_arch = "x86_64")]
    pub fn div(self, other: Self) -> Self {
        arithmetic::div(self, other)
    }

    /// The square root, correctly rounded.
    #[cfg(target_arch = "x86_64")]
    pub fn sqrt(self) -> Self {
        arithmetic::sqrt(self)
    }

    /// `x mod y` with the sign of `x` (`fmod`).
    #[cfg(target_arch = "x86_64")]
    pub fn fmod(self, other: Self) -> Self {
        arithmetic::fmod(self, other)
    }

    /// `floor`.
    #[cfg(target_arch = "x86_64")]
    pub fn floor(self) -> Self {
        arithmetic::round_integral(self, arithmetic::Mode::Floor)
    }

    /// `ceil`.
    #[cfg(target_arch = "x86_64")]
    pub fn ceil(self) -> Self {
        arithmetic::round_integral(self, arithmetic::Mode::Ceil)
    }

    /// `trunc`.
    #[cfg(target_arch = "x86_64")]
    pub fn trunc(self) -> Self {
        arithmetic::round_integral(self, arithmetic::Mode::Trunc)
    }

    /// `round`: halfway cases away from zero.
    #[cfg(target_arch = "x86_64")]
    pub fn round(self) -> Self {
        arithmetic::round_integral(self, arithmetic::Mode::Round)
    }

    /// `nextafter` towards `other`.
    #[cfg(target_arch = "x86_64")]
    pub fn next_after(self, other: Self) -> Self {
        arithmetic::next_after(self, other)
    }

    /// `roundeven`: round to the nearest integer with ties to even (C23).
    #[cfg(target_arch = "x86_64")]
    pub fn roundeven(self) -> Self {
        arithmetic::round_integral(self, arithmetic::Mode::Even)
    }

    /// `remainder`: `x - n*y` with `n` the nearest even integer to `x/y`.
    ///
    /// Exact while `|x/y|` fits the significand (the same bound as [`LongDouble::fmod`]).
    #[cfg(target_arch = "x86_64")]
    pub fn remainder(self, other: Self) -> Self {
        self.remquo(other).0
    }

    /// `remquo`: the remainder and the low bits of the quotient with its sign.
    #[cfg(target_arch = "x86_64")]
    pub fn remquo(self, other: Self) -> (Self, i32) {
        if self.is_nan() || other.is_nan() || self.is_infinite() || other.is_zero() {
            return (LongDouble::nan(), 0);
        }
        if other.is_infinite() || self.is_zero() {
            return (self, 0);
        }
        let quotient = self.div(other).roundeven();
        let magnitude = quotient.abs().to_u64().unwrap_or(0) & 0x7f;
        let negative = self.sign_bit() ^ other.sign_bit();
        let low = if negative {
            -(magnitude as i32)
        } else {
            magnitude as i32
        };
        let remainder = self.sub(quotient.mul(other));
        (remainder, low)
    }

    /// `fdim`: the positive difference.
    #[cfg(target_arch = "x86_64")]
    pub fn fdim(self, other: Self) -> Self {
        if self.is_nan() || other.is_nan() {
            return LongDouble::nan();
        }
        if self.total_cmp(other) == Ordering::Greater {
            self.sub(other)
        } else {
            LongDouble::zero(false)
        }
    }

    /// `fmax`: the larger value, with `+0` winning over `-0` and a NaN yielding the other operand.
    #[cfg(target_arch = "x86_64")]
    pub fn fmax(self, other: Self) -> Self {
        if self.is_nan() {
            return other;
        }
        if other.is_nan() {
            return self;
        }
        if self.total_cmp(other) == Ordering::Greater {
            self
        } else {
            other
        }
    }

    /// `fmin`: the smaller value, with `-0` winning over `+0` and a NaN yielding the other operand.
    #[cfg(target_arch = "x86_64")]
    pub fn fmin(self, other: Self) -> Self {
        if self.is_nan() {
            return other;
        }
        if other.is_nan() {
            return self;
        }
        if self.total_cmp(other) == Ordering::Less {
            self
        } else {
            other
        }
    }

    /// Bridge from C42's `raven_libc_dtoa::LongDouble` byte image (the gdtoa `strtold` output).
    ///
    /// The shell parses with C42 and converts here; this crate never parses.
    pub fn from_dtoa_bytes(bytes: [u8; 16]) -> Self {
        Self::from_bits(u128::from_le_bytes(bytes))
    }

    /// Bridge to C42's `raven_libc_dtoa::LongDouble` byte image.
    pub fn to_dtoa_bytes(self) -> [u8; 16] {
        self.to_bits().to_le_bytes()
    }

    /// `scalbn`: `x * 2^n` by exponent adjustment, with overflow and subnormal rounding.
    #[cfg(target_arch = "x86_64")]
    pub fn scalbn(self, n: i32) -> Self {
        if !self.is_finite() || self.is_zero() {
            return self;
        }
        let negative = self.sign_bit();
        let field = self.exponent_field() as i32 + n;
        let mut significand = self.significand_bits();
        if field >= 0x7fff {
            return LongDouble::infinity(negative);
        }
        if field > 0 {
            return LongDouble::from_parts(negative, field as u32, significand);
        }
        // Subnormal: shift the significand right by the missing exponent, with rounding.
        let shift = (1 - field) as u32;
        if shift >= 64 {
            return LongDouble::zero(negative);
        }
        let lost = significand & ((1u64 << shift) - 1);
        let half = 1u64 << (shift - 1);
        significand >>= shift;
        if lost > half || (lost == half && (significand & 1) != 0) {
            significand += 1;
        }
        LongDouble::from_parts(negative, 0, significand)
    }
}

/// Monotone key for the total order over the 80 meaningful bits: negatives are complemented, so
/// `-NaN < -inf < -0 < +0 < +inf < +NaN`. The six padding bytes never enter the key.
fn order_key(x: LongDouble) -> u128 {
    const VALUE_MASK: u128 = (1u128 << 80) - 1;
    let bits = x.to_bits() & VALUE_MASK;
    if bits & SIGN_MASK != 0 {
        !bits & VALUE_MASK
    } else {
        bits | SIGN_MASK
    }
}

/// Build an exact value from a 64-bit significand whose bit 63 is set (or the value is zero).
fn pack_exact(negative: bool, exp: i32, significand: u64) -> LongDouble {
    if significand == 0 {
        return LongDouble::zero(negative);
    }
    // value = significand * 2^exp, bit 63 set.
    let field = exp + 63 + EXP_BIAS;
    if field <= 0 {
        let shift = (1 - field) as u32;
        if shift >= 64 {
            return LongDouble::zero(negative);
        }
        return LongDouble::from_parts(negative, 0, significand >> shift);
    }
    if field >= EXP_MASK as i32 {
        return LongDouble::infinity(negative);
    }
    LongDouble::from_parts(negative, field as u32, significand)
}

/// The x86-64 arithmetic engine.
#[cfg(target_arch = "x86_64")]
mod arithmetic {
    use super::{EXP_BIAS, EXP_MASK, INT_BIT, LongDouble, SIGN_MASK};

    /// The integral rounding helpers.
    pub(super) enum Mode {
        Floor,
        Ceil,
        Trunc,
        Round,
        Even,
    }

    struct Finite {
        negative: bool,
        /// Unbiased exponent with the value equal to `significand * 2^(exponent - 63)`.
        exponent: i32,
        significand: u64,
    }

    /// Unpack a finite, non-zero value with the integer bit set.
    fn finite(x: LongDouble) -> Option<Finite> {
        if !x.is_finite() || x.is_zero() {
            return None;
        }
        let negative = x.sign_bit();
        let mut exponent = x.exponent_field() as i32;
        let mut significand = x.significand_bits();
        if exponent == 0 {
            while significand & INT_BIT == 0 {
                significand <<= 1;
                exponent -= 1;
            }
            exponent += 1;
        }
        Some(Finite {
            negative,
            exponent: exponent - EXP_BIAS - 63,
            significand,
        })
    }

    fn negate(x: LongDouble) -> LongDouble {
        LongDouble::from_bits(x.to_bits() ^ SIGN_MASK)
    }

    pub(super) fn add(a: LongDouble, b: LongDouble, subtract: bool) -> LongDouble {
        if a.is_nan() || b.is_nan() {
            return LongDouble::nan();
        }
        if a.is_infinite() || b.is_infinite() {
            let b_negative = b.sign_bit() ^ subtract;
            if a.is_infinite() && b.is_infinite() {
                return if a.sign_bit() != b_negative {
                    LongDouble::nan()
                } else {
                    a
                };
            }
            if a.is_infinite() {
                return a;
            }
            return if b_negative { negate(b) } else { b };
        }
        if a.is_zero() && b.is_zero() {
            let negative = a.sign_bit() == (b.sign_bit() ^ subtract);
            return LongDouble::zero(negative);
        }
        if b.is_zero() {
            return if subtract { negate(a) } else { a };
        }
        if a.is_zero() {
            return if subtract { negate(b) } else { b };
        }
        let left = finite(a).expect("finite");
        let right = finite(b).expect("finite");
        let right_negative = right.negative ^ subtract;
        let (large_exp, large_sig, large_neg, small_exp, small_sig, small_neg) =
            if (left.exponent, left.significand) >= (right.exponent, right.significand) {
                (
                    left.exponent,
                    left.significand,
                    left.negative,
                    right.exponent,
                    right.significand,
                    right_negative,
                )
            } else {
                (
                    right.exponent,
                    right.significand,
                    right_negative,
                    left.exponent,
                    left.significand,
                    left.negative,
                )
            };
        let difference = (large_exp - small_exp) as u32;
        let mut scaled = (small_sig as u128) << 3;
        let mut sticky = false;
        if difference > 0 {
            if difference >= 64 {
                sticky = small_sig != 0;
                scaled = 0;
            } else {
                sticky = small_sig & ((1u64 << difference) - 1) != 0;
                scaled >>= difference;
            }
        }
        let mut significand = (large_sig as u128) << 3;
        let negative;
        if large_neg == small_neg {
            significand += scaled;
            if sticky {
                significand |= 1;
            }
            negative = large_neg;
        } else if scaled > significand {
            significand = scaled - significand;
            negative = small_neg;
        } else {
            significand -= scaled;
            if sticky {
                significand = significand.saturating_sub(1);
            }
            negative = large_neg;
        }
        round_pack(negative, large_exp - 3, significand, sticky)
    }

    pub(super) fn mul(a: LongDouble, b: LongDouble) -> LongDouble {
        if a.is_nan() || b.is_nan() {
            return LongDouble::nan();
        }
        let negative = a.sign_bit() ^ b.sign_bit();
        if a.is_infinite() || b.is_infinite() {
            if a.is_zero() || b.is_zero() {
                return LongDouble::nan();
            }
            return LongDouble::infinity(negative);
        }
        if a.is_zero() || b.is_zero() {
            return LongDouble::zero(negative);
        }
        let left = finite(a).expect("finite");
        let right = finite(b).expect("finite");
        let product = (left.significand as u128) * (right.significand as u128);
        round_pack(negative, left.exponent + right.exponent, product, false)
    }

    pub(super) fn div(a: LongDouble, b: LongDouble) -> LongDouble {
        if a.is_nan() || b.is_nan() {
            return LongDouble::nan();
        }
        let negative = a.sign_bit() ^ b.sign_bit();
        if a.is_infinite() {
            if b.is_infinite() {
                return LongDouble::nan();
            }
            return LongDouble::infinity(negative);
        }
        if b.is_infinite() {
            return LongDouble::zero(negative);
        }
        if b.is_zero() {
            if a.is_zero() {
                return LongDouble::nan();
            }
            return LongDouble::infinity(negative);
        }
        if a.is_zero() {
            return LongDouble::zero(negative);
        }
        let left = finite(a).expect("finite");
        let right = finite(b).expect("finite");
        let numerator = (left.significand as u128) << 64;
        let divisor = right.significand as u128;
        round_pack(
            negative,
            left.exponent - right.exponent - 64,
            numerator / divisor,
            !numerator.is_multiple_of(divisor),
        )
    }

    pub(super) fn sqrt(a: LongDouble) -> LongDouble {
        if a.is_nan() {
            return LongDouble::nan();
        }
        if a.is_zero() {
            return a;
        }
        if a.sign_bit() {
            return LongDouble::nan();
        }
        if a.is_infinite() {
            return a;
        }
        let value = finite(a).expect("finite");
        // value = significand * 2^exponent. An even exponent keeps the 64-bit significand and
        // squares with `sig << 64`; an odd one uses `sig << 63`, so both radicands stay inside
        // u128 and every root keeps at least 62 bits of precision.
        let (radicand, exponent) = if value.exponent % 2 == 0 {
            ((value.significand as u128) << 64, (value.exponent - 64) / 2)
        } else {
            ((value.significand as u128) << 63, (value.exponent - 63) / 2)
        };
        let root = isqrt(radicand);
        round_pack(false, exponent, root, root * root != radicand)
    }

    fn isqrt(value: u128) -> u128 {
        if value == 0 {
            return 0;
        }
        let mut x = value;
        let mut y = (x + 1) / 2;
        while y < x {
            x = y;
            y = (x + value / x) / 2;
        }
        x
    }

    /// Round `significand * 2^exponent` to the 64-bit x87 significand (nearest even).
    fn round_pack(
        negative: bool,
        exponent: i32,
        significand: u128,
        mut sticky: bool,
    ) -> LongDouble {
        if significand == 0 {
            return LongDouble::zero(negative);
        }
        let highest = 127 - significand.leading_zeros();
        let mut exponent = exponent;
        if highest > 63 {
            let drop = highest - 63;
            let guard = (significand >> (drop - 1)) & 1 != 0;
            let round = drop >= 2 && (significand >> (drop - 2)) & 1 != 0;
            let rest = if drop >= 2 {
                significand & ((1u128 << (drop - 2)) - 1) != 0
            } else {
                false
            };
            sticky |= rest;
            let mut kept = significand >> drop;
            let low = kept & 1 != 0;
            exponent += drop as i32;
            if guard && (round || sticky || low) {
                kept += 1;
                if kept >> 64 != 0 {
                    kept >>= 1;
                    exponent += 1;
                }
            }
            pack_checked(negative, exponent, kept as u64)
        } else {
            let shift = (63 - highest) as i32;
            pack_checked(negative, exponent - shift, (significand << shift) as u64)
        }
    }

    fn pack_checked(negative: bool, exponent: i32, significand: u64) -> LongDouble {
        let field = exponent + 63 + EXP_BIAS;
        if field <= 0 {
            let shift = (1 - field) as u32;
            if shift >= 64 {
                return LongDouble::zero(negative);
            }
            let lost = significand & ((1u64 << shift) - 1);
            let half = 1u64 << (shift - 1);
            let mut sub = significand >> shift;
            if lost > half || (lost == half && (sub & 1) != 0) {
                sub += 1;
            }
            return LongDouble::from_parts(negative, 0, sub);
        }
        if field >= EXP_MASK as i32 {
            return LongDouble::infinity(negative);
        }
        LongDouble::from_parts(negative, field as u32, significand)
    }

    pub(super) fn round_integral(x: LongDouble, mode: Mode) -> LongDouble {
        if !x.is_finite() || x.is_zero() {
            return x;
        }
        let negative = x.sign_bit();
        let value = finite(x).expect("finite");
        // value = significand * 2^exponent, significand in [2^63, 2^64).
        let exponent = value.exponent;
        if exponent >= 0 {
            return x;
        }
        let round_up = match mode {
            Mode::Floor => negative,
            Mode::Ceil => !negative,
            Mode::Trunc | Mode::Even => false,
            Mode::Round => exponent == -64,
        };
        if exponent <= -64 {
            // |x| < 0.5 at E < -64; exactly [0.5, 1) at E == -64.
            return if round_up {
                LongDouble::one().copysign(x)
            } else {
                LongDouble::zero(negative)
            };
        }
        let drop = (-exponent) as u32;
        let kept = (value.significand >> drop) << drop;
        let lost = value.significand & ((1u64 << drop) - 1);
        if lost == 0 {
            return x;
        }
        let increment = match mode {
            Mode::Floor => negative,
            Mode::Ceil => !negative,
            Mode::Trunc => false,
            Mode::Round => {
                let half = 1u64 << (drop - 1);
                lost >= half
            }
            Mode::Even => {
                let half = 1u64 << (drop - 1);
                lost > half || (lost == half && ((kept >> drop) & 1) != 0)
            }
        };
        // The increment can carry out of the 64-bit significand (e.g. 1.5 -> 2.0); then the
        // significand restarts at 2^63 with one higher exponent.
        let (magnitude, carry) = if increment {
            match kept.checked_add(1u64 << drop) {
                Some(value) => (value, 0),
                None => (1u64 << 63, 1),
            }
        } else {
            (kept, 0)
        };
        LongDouble::from_parts(
            negative,
            (exponent + EXP_BIAS + 63 + carry) as u32,
            magnitude,
        )
    }

    pub(super) fn fmod(a: LongDouble, b: LongDouble) -> LongDouble {
        if a.is_nan() || b.is_nan() || a.is_infinite() || b.is_zero() {
            return LongDouble::nan();
        }
        if b.is_infinite() || a.is_zero() {
            return a;
        }
        // `a - trunc(a/b) * b`: exact while the quotient fits the significand (|a/b| < 2^64),
        // which is every case the exact-class tests and the shell need; beyond that the quotient
        // is truncated by the soft division and the remainder loses low bits (documented in the
        // C47c report).
        let quotient = round_integral(div(a, b), Mode::Trunc);
        let product = mul(quotient, b);
        add(a, product, true)
    }

    pub(super) fn next_after(x: LongDouble, towards: LongDouble) -> LongDouble {
        if x.is_nan() || towards.is_nan() {
            return LongDouble::nan();
        }
        let key = super::order_key(x);
        let target = super::order_key(towards);
        if key == target {
            return towards;
        }
        let next = if key < target { key + 1 } else { key - 1 };
        const VALUE_MASK: u128 = (1u128 << 80) - 1;
        let bits = if next & SIGN_MASK != 0 {
            next ^ SIGN_MASK
        } else {
            !next & VALUE_MASK
        };
        LongDouble::from_bits(bits)
    }
}

/// `fabsl`.
pub fn fabsl(x: LongDouble) -> LongDouble {
    x.abs()
}

/// `copysignl`.
pub fn copysignl(x: LongDouble, sign: LongDouble) -> LongDouble {
    x.copysign(sign)
}

/// `isnanl`.
pub fn isnanl(x: LongDouble) -> bool {
    x.is_nan()
}

/// `isfinitel`.
pub fn isfinitel(x: LongDouble) -> bool {
    x.is_finite()
}

/// `isinfl`.
pub fn isinfl(x: LongDouble) -> bool {
    x.is_infinite()
}

/// `isnormall`.
pub fn isnormall(x: LongDouble) -> bool {
    x.is_normal()
}

/// `signbitl`.
pub fn signbitl(x: LongDouble) -> bool {
    x.sign_bit()
}

/// `fpclassifyl`.
pub fn fpclassifyl(x: LongDouble) -> i32 {
    x.class()
}

/// `sqrtl`.
#[cfg(target_arch = "x86_64")]
pub fn sqrtl(x: LongDouble) -> LongDouble {
    x.sqrt()
}

/// `floorl`.
#[cfg(target_arch = "x86_64")]
pub fn floorl(x: LongDouble) -> LongDouble {
    x.floor()
}

/// `ceill`.
#[cfg(target_arch = "x86_64")]
pub fn ceill(x: LongDouble) -> LongDouble {
    x.ceil()
}

/// `truncl`.
#[cfg(target_arch = "x86_64")]
pub fn truncl(x: LongDouble) -> LongDouble {
    x.trunc()
}

/// `roundl`.
#[cfg(target_arch = "x86_64")]
pub fn roundl(x: LongDouble) -> LongDouble {
    x.round()
}

/// `fmodl`.
#[cfg(target_arch = "x86_64")]
pub fn fmodl(x: LongDouble, y: LongDouble) -> LongDouble {
    x.fmod(y)
}

/// `nextafterl`.
#[cfg(target_arch = "x86_64")]
pub fn nextafterl(x: LongDouble, y: LongDouble) -> LongDouble {
    x.next_after(y)
}

/// `nexttowardl`; the hidden-pointer argument thunk of RD-33 is the export's job (C60).
#[cfg(target_arch = "x86_64")]
pub fn nexttowardl(x: LongDouble, y: LongDouble) -> LongDouble {
    x.next_after(y)
}

/// `totalorderl`.
pub fn totalorderl(x: LongDouble, y: LongDouble) -> bool {
    x.total_cmp(y) != Ordering::Greater
}

/// `remainderl`.
#[cfg(target_arch = "x86_64")]
pub fn remainderl(x: LongDouble, y: LongDouble) -> LongDouble {
    x.remainder(y)
}

/// `remquol`: the remainder plus the quotient low bits.
#[cfg(target_arch = "x86_64")]
pub fn remquol(x: LongDouble, y: LongDouble) -> (LongDouble, i32) {
    x.remquo(y)
}

/// `fdiml`.
#[cfg(target_arch = "x86_64")]
pub fn fdiml(x: LongDouble, y: LongDouble) -> LongDouble {
    x.fdim(y)
}

/// `fmaxl`.
#[cfg(target_arch = "x86_64")]
pub fn fmaxl(x: LongDouble, y: LongDouble) -> LongDouble {
    x.fmax(y)
}

/// `fminl`.
#[cfg(target_arch = "x86_64")]
pub fn fminl(x: LongDouble, y: LongDouble) -> LongDouble {
    x.fmin(y)
}

/// `scalbnl`.
#[cfg(target_arch = "x86_64")]
pub fn scalbnl(x: LongDouble, n: i32) -> LongDouble {
    x.scalbn(n)
}

/// `rintl`: round to the nearest integer, ties to even (the default rounding mode).
#[cfg(target_arch = "x86_64")]
pub fn rintl(x: LongDouble) -> LongDouble {
    x.roundeven()
}

/// `nearbyintl`: as `rintl` (the skeleton does not raise the inexact flag differently).
#[cfg(target_arch = "x86_64")]
pub fn nearbyintl(x: LongDouble) -> LongDouble {
    x.roundeven()
}

/// `roundevenl` (C23).
#[cfg(target_arch = "x86_64")]
pub fn roundevenl(x: LongDouble) -> LongDouble {
    x.roundeven()
}

/// `lrintl`: `rintl` as an `i64`.
#[cfg(target_arch = "x86_64")]
pub fn lrintl(x: LongDouble) -> i64 {
    x.roundeven().to_i64().unwrap_or(i64::MIN)
}

/// `llrintl`: `rintl` as an `i64`.
#[cfg(target_arch = "x86_64")]
pub fn llrintl(x: LongDouble) -> i64 {
    lrintl(x)
}

/// `lroundl`: halfway cases away from zero, as an `i64`.
#[cfg(target_arch = "x86_64")]
pub fn lroundl(x: LongDouble) -> i64 {
    x.round().to_i64().unwrap_or(i64::MIN)
}

/// `llroundl`: halfway cases away from zero, as an `i64`.
#[cfg(target_arch = "x86_64")]
pub fn llroundl(x: LongDouble) -> i64 {
    lroundl(x)
}

/// `nanl`: a quiet NaN (the C tag argument is not represented in Rust).
pub fn nanl() -> LongDouble {
    LongDouble::nan()
}

#[cfg(test)]
#[path = "long_double_ut.rs"]
mod tests;
