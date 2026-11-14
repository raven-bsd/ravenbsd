// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! IEEE binary128 bit rules (the `long double` format of aarch64 and riscv64).
//!
//! Stable Rust has no `f128`, so the format is handled as its 128-bit pattern: 1 sign bit at bit
//! 127, 15 exponent bits, a 112-bit stored significand and an implicit integer bit.
//! This module implements the classification, the total order and the `f64` conversions; the
//! 128-bit arithmetic and the `LongDouble` wiring for those targets are the remaining step listed
//! in the C47c report.

use crate::consts::{FP_INFINITE, FP_NAN, FP_NORMAL, FP_SUBNORMAL, FP_ZERO};

/// Sign bit of the binary128 pattern.
pub const SIGN: u128 = 1 << 127;
/// Exponent field mask.
pub const EXPONENT_MASK: u32 = 0x7fff;
/// Shift of the exponent field.
pub const EXPONENT_SHIFT: u32 = 112;
/// Stored significand mask (112 bits).
pub const SIGNIFICAND_MASK: u128 = (1u128 << 112) - 1;
/// The hidden integer bit.
pub const HIDDEN: u128 = 1 << 112;
/// The quiet bit of the stored significand.
pub const QUIET: u128 = 1 << 111;
/// Exponent bias.
pub const BIAS: i32 = 16383;

/// `fpclassify` for a raw binary128 pattern.
pub fn classify(bits: u128) -> i32 {
    let exponent = ((bits >> EXPONENT_SHIFT) & EXPONENT_MASK as u128) as u32;
    let significand = bits & SIGNIFICAND_MASK;
    if exponent == EXPONENT_MASK {
        if significand == 0 {
            FP_INFINITE
        } else {
            FP_NAN
        }
    } else if exponent == 0 {
        if significand == 0 {
            FP_ZERO
        } else {
            FP_SUBNORMAL
        }
    } else {
        FP_NORMAL
    }
}

/// The monotone key of the total order.
pub fn order_key(bits: u128) -> u128 {
    if bits & SIGN != 0 { !bits } else { bits | SIGN }
}

/// C23 `totalorder` on two patterns.
pub fn total_order(a: u128, b: u128) -> bool {
    order_key(a) <= order_key(b)
}

/// The pattern as an `f64`, correctly rounded (round to nearest even).
pub fn to_f64(bits: u128) -> f64 {
    let negative = bits & SIGN != 0;
    let sign = if negative { 1u64 << 63 } else { 0 };
    match classify(bits) {
        FP_NAN => return f64::NAN,
        FP_INFINITE => return f64::from_bits(sign | (0x7ffu64 << 52)),
        FP_ZERO => return f64::from_bits(sign),
        _ => {}
    }
    let field = ((bits >> EXPONENT_SHIFT) & EXPONENT_MASK as u128) as i32;
    let mut significand = bits & SIGNIFICAND_MASK;
    let mut exponent = if field == 0 { 1 - BIAS } else { field - BIAS };
    if field != 0 {
        significand |= HIDDEN;
    }
    while significand & HIDDEN == 0 {
        significand <<= 1;
        exponent -= 1;
    }
    // value = significand * 2^(exponent - 112)
    let drop = 113 - 53;
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
    if exponent > 1023 {
        return f64::from_bits(sign | (0x7ffu64 << 52));
    }
    if exponent < -1022 {
        let shift = -1022 - exponent;
        if shift > 53 {
            return f64::from_bits(sign);
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

/// An `f64` as a binary128 pattern (exact).
pub fn from_f64(value: f64) -> u128 {
    let bits = value.to_bits();
    let negative = bits >> 63 != 0;
    let sign = if negative { SIGN } else { 0 };
    let field = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1u64 << 52) - 1);
    if field == 0x7ff {
        return if fraction != 0 {
            sign | (EXPONENT_MASK as u128) << EXPONENT_SHIFT | QUIET
        } else {
            sign | (EXPONENT_MASK as u128) << EXPONENT_SHIFT
        };
    }
    if field == 0 && fraction == 0 {
        return sign;
    }
    let (mantissa, exponent) = if field == 0 {
        (fraction, -1074)
    } else {
        (fraction | (1u64 << 52), field - 1023 - 52)
    };
    // value = mantissa * 2^exponent. Move the top bit of `mantissa` to the hidden bit at 112; the
    // value is then `stored * 2^(exponent - (112 - msb))`, so the f128 field is exponent + msb + BIAS.
    let msb = 63 - mantissa.leading_zeros() as i32;
    let stored = (mantissa as u128) << (112 - msb as u32);
    let field = exponent + msb + BIAS;
    sign | (((field as u128) & EXPONENT_MASK as u128) << EXPONENT_SHIFT)
        | (stored & SIGNIFICAND_MASK)
}

/// A binary128 value with the IEEE 754-2019 semantics for the aarch64/riscv64 `long double`.
///
/// The classification, ordering, conversions and `add`/`sub` are implemented; `mul`, `div` and
/// `sqrt` (which need a 226-bit intermediate) are the remaining item, listed in the C47c report.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct LongDouble128 {
    bits: u128,
}

impl LongDouble128 {
    /// From the raw pattern.
    pub const fn from_bits(bits: u128) -> Self {
        Self { bits }
    }

    /// The raw pattern.
    pub const fn to_bits(self) -> u128 {
        self.bits
    }

    /// Signed zero.
    pub const fn zero(negative: bool) -> Self {
        Self {
            bits: if negative { SIGN } else { 0 },
        }
    }

    /// One.
    pub const fn one() -> Self {
        Self {
            bits: 0x3fff_0000_0000_0000_0000_0000_0000_0000,
        }
    }

    /// A quiet NaN.
    pub const fn nan() -> Self {
        Self {
            bits: (EXPONENT_MASK as u128) << EXPONENT_SHIFT | QUIET,
        }
    }

    /// An infinity.
    pub const fn infinity(negative: bool) -> Self {
        let sign = if negative { SIGN } else { 0 };
        Self {
            bits: sign | (EXPONENT_MASK as u128) << EXPONENT_SHIFT,
        }
    }

    /// The sign bit.
    pub const fn sign_bit(self) -> bool {
        self.bits & SIGN != 0
    }

    /// The biased exponent field.
    pub const fn exponent_field(self) -> u32 {
        ((self.bits >> EXPONENT_SHIFT) & EXPONENT_MASK as u128) as u32
    }

    /// The stored significand (112 bits, hidden bit implicit).
    pub const fn significand_bits(self) -> u128 {
        self.bits & SIGNIFICAND_MASK
    }

    /// The `fpclassify` class.
    pub fn class(self) -> i32 {
        classify(self.bits)
    }

    /// Whether the value is a NaN.
    pub fn is_nan(self) -> bool {
        classify(self.bits) == FP_NAN
    }

    /// Whether the value is infinite.
    pub fn is_infinite(self) -> bool {
        classify(self.bits) == FP_INFINITE
    }

    /// Whether the value is finite.
    pub fn is_finite(self) -> bool {
        matches!(classify(self.bits), FP_ZERO | FP_SUBNORMAL | FP_NORMAL)
    }

    /// Whether the value is zero.
    pub fn is_zero(self) -> bool {
        classify(self.bits) == FP_ZERO
    }

    /// The total order.
    pub fn total_cmp(self, other: Self) -> core::cmp::Ordering {
        order_key(self.bits).cmp(&order_key(other.bits))
    }

    /// The `f64` value, correctly rounded.
    pub fn to_f64(self) -> f64 {
        to_f64(self.bits)
    }

    /// From an `f64` (exact).
    pub fn from_f64(value: f64) -> Self {
        Self {
            bits: from_f64(value),
        }
    }

    /// `x + y`, correctly rounded.
    #[allow(clippy::should_implement_trait)] // a soft value type: the operators are the C API
    pub fn add(self, other: Self) -> Self {
        Self {
            bits: add_bits(self.bits, other.bits, false),
        }
    }

    /// `x - y`, correctly rounded.
    #[allow(clippy::should_implement_trait)]
    pub fn sub(self, other: Self) -> Self {
        Self {
            bits: add_bits(self.bits, other.bits, true),
        }
    }

    /// `x * y`, correctly rounded.
    #[allow(clippy::should_implement_trait)]
    pub fn mul(self, other: Self) -> Self {
        Self {
            bits: mul128_bits(self.bits, other.bits),
        }
    }

    /// `x / y`, correctly rounded.
    #[allow(clippy::should_implement_trait)]
    pub fn div(self, other: Self) -> Self {
        Self {
            bits: div128_bits(self.bits, other.bits),
        }
    }

    /// The square root, correctly rounded.
    pub fn sqrt(self) -> Self {
        Self {
            bits: sqrt128_bits(self.bits),
        }
    }

    /// `|x|`.
    pub fn abs(self) -> Self {
        Self {
            bits: self.bits & !SIGN,
        }
    }

    /// `x` with the sign of `sign`.
    pub fn copysign(self, sign: Self) -> Self {
        Self {
            bits: (self.bits & !SIGN) | (sign.bits & SIGN),
        }
    }

    /// A signed integer, exactly.
    pub fn from_i64(value: i64) -> Self {
        if value == 0 {
            return Self::zero(false);
        }
        let negative = value < 0;
        let magnitude = value.unsigned_abs();
        let msb = 63 - magnitude.leading_zeros();
        let significand = (magnitude as u128) << (112 - msb);
        let sign = if negative { SIGN } else { 0 };
        Self {
            bits: sign
                | (((msb as i32 + BIAS) as u128 & EXPONENT_MASK as u128) << EXPONENT_SHIFT)
                | (significand & SIGNIFICAND_MASK),
        }
    }

    /// Truncate toward zero as `i64`; `None` when it does not fit or is not finite.
    pub fn to_i64(self) -> Option<i64> {
        if !self.is_finite() || self.is_zero() {
            return if self.is_zero() { Some(0) } else { None };
        }
        let (exponent, significand) = unpack(self.bits);
        if exponent < 0 {
            return Some(0);
        }
        if exponent > 63 {
            return None;
        }
        let magnitude = significand >> (112 - exponent);
        let magnitude = u64::try_from(magnitude).ok()?;
        let value = i64::try_from(magnitude).ok()?;
        Some(if self.sign_bit() { -value } else { value })
    }

    /// `round`: halfway cases away from zero.
    pub fn round(self) -> Self {
        if !self.is_finite() || self.is_zero() {
            return self;
        }
        let (exponent, significand) = unpack(self.bits);
        let negative = self.sign_bit();
        if exponent >= 112 {
            return self;
        }
        if exponent < -1 {
            return Self::zero(negative);
        }
        if exponent == -1 {
            return if negative {
                Self::zero(true).sub(Self::one())
            } else {
                Self::one()
            };
        }
        let drop = (112 - exponent) as u32;
        let kept = (significand >> drop) << drop;
        let lost = significand & ((1u128 << drop) - 1);
        if lost == 0 {
            return self;
        }
        let half = 1u128 << (drop - 1);
        let magnitude = if lost >= half {
            kept + (1u128 << drop)
        } else {
            kept
        };
        Self {
            bits: from_parts_bits(negative, exponent, magnitude),
        }
    }

    /// `scalbn`: `x * 2^n` by exponent adjustment, with overflow and subnormal rounding.
    pub fn scalbn(self, n: i32) -> Self {
        if !self.is_finite() || self.is_zero() {
            return self;
        }
        let negative = self.sign_bit();
        let field = self.exponent_field() as i32 + n;
        let mut significand = self.significand_bits() | HIDDEN;
        let sign = if negative { SIGN } else { 0 };
        if field >= EXPONENT_MASK as i32 {
            return Self::infinity(negative);
        }
        if field > 0 {
            return Self {
                bits: sign
                    | (((field as u128) & EXPONENT_MASK as u128) << EXPONENT_SHIFT)
                    | (significand & SIGNIFICAND_MASK),
            };
        }
        let shift = (1 - field) as u32;
        if shift >= 113 {
            return Self::zero(negative);
        }
        let lost = significand & ((1u128 << shift) - 1);
        let half = 1u128 << (shift - 1);
        significand >>= shift;
        if lost > half || (lost == half && (significand & 1) != 0) {
            significand += 1;
        }
        Self {
            bits: sign | (significand & SIGNIFICAND_MASK),
        }
    }

    /// The value scaled to `[1, 2)` (the mantissa used by the logarithm reduction).
    pub fn mantissa(self) -> Self {
        Self {
            bits: ((BIAS as u128) << EXPONENT_SHIFT) | self.significand_bits(),
        }
    }
}

/// Build a value from a sign, the unpacked exponent and a significand normalised at bit 112.
fn from_parts_bits(negative: bool, exponent: i32, significand: u128) -> u128 {
    let sign = if negative { SIGN } else { 0 };
    let mut significand = significand;
    let mut exponent = exponent;
    if significand >> 113 != 0 {
        significand >>= 1;
        exponent += 1;
    }
    sign | ((((exponent + BIAS) as u128) & EXPONENT_MASK as u128) << EXPONENT_SHIFT)
        | (significand & SIGNIFICAND_MASK)
}

/// `x / y` over the 113-bit significands: a restoring division of `a_sig << 113` by `b_sig`.
pub fn div128_bits(a: u128, b: u128) -> u128 {
    let a_class = classify(a);
    let b_class = classify(b);
    let negative = (a & SIGN != 0) ^ (b & SIGN != 0);
    if a_class == FP_NAN || b_class == FP_NAN {
        return nan_bits();
    }
    if a_class == FP_INFINITE {
        if b_class == FP_INFINITE {
            return nan_bits();
        }
        return infinity_bits(negative);
    }
    if b_class == FP_INFINITE {
        return if negative { SIGN } else { 0 };
    }
    if b_class == FP_ZERO {
        if a_class == FP_ZERO {
            return nan_bits();
        }
        return infinity_bits(negative);
    }
    if a_class == FP_ZERO {
        return if negative { SIGN } else { 0 };
    }
    let (a_exp, a_sig) = unpack(a);
    let (b_exp, b_sig) = unpack(b);
    let mut limbs = [0u64; 4];
    add_shifted(&mut limbs, a_sig, 113);
    let divisor = b_sig;
    let mut remainder = 0u128;
    let mut quotient = 0u128;
    for index in (0..=225).rev() {
        remainder = (remainder << 1) | u128::from(limb_bit(&limbs, index));
        let subtract = remainder >= divisor;
        if subtract {
            remainder -= divisor;
        }
        // The quotient is built MSB-first; the first meaningful bit lands at index 113 and the
        // accumulator keeps at most 114 bits.
        if index <= 113 {
            quotient = (quotient << 1) | u128::from(subtract);
        }
    }
    // value = (quotient + remainder/divisor) * 2^(a_exp - b_exp - 113)
    let significand = (quotient << 3) | u128::from(remainder != 0);
    pack_binary128(negative, a_exp - b_exp - 113 - 3, significand)
}

/// The square root over the 113-bit significand: a restoring integer square root of `sig << 114`.
pub fn sqrt128_bits(a: u128) -> u128 {
    let class = classify(a);
    if class == FP_NAN {
        return nan_bits();
    }
    if a & SIGN != 0 {
        if class == FP_ZERO {
            return a;
        }
        return nan_bits();
    }
    if class == FP_INFINITE || class == FP_ZERO {
        return a;
    }
    let (exponent, significand) = unpack(a);
    let mut exponent = exponent;
    let mut significand = significand;
    if (exponent - 112) % 2 != 0 {
        significand <<= 1;
        exponent -= 1;
    }
    let mut radicand = [0u64; 4];
    add_shifted(&mut radicand, significand, 114);
    // Digit-by-digit square root over bit pairs; the running remainder stays below 2^115.
    let mut root = 0u128;
    let mut remainder = 0u128;
    for pair in (0..=113u32).rev() {
        let bits = (limb_bit(&radicand, 2 * pair + 1) << 1) | limb_bit(&radicand, 2 * pair);
        remainder = (remainder << 2) | u128::from(bits);
        let trial = (root << 2) | 1;
        if remainder >= trial {
            remainder -= trial;
            root = (root << 1) | 1;
        } else {
            root <<= 1;
        }
    }
    let significand = (root << 3) | u128::from(remainder != 0);
    pack_binary128(false, (exponent - 112) / 2 - 57 - 3, significand)
}

fn infinity_bits(negative: bool) -> u128 {
    let sign = if negative { SIGN } else { 0 };
    sign | (EXPONENT_MASK as u128) << EXPONENT_SHIFT
}

fn limb_bit(limbs: &[u64; 4], index: u32) -> u64 {
    let word = (index / 64) as usize;
    let bit = index % 64;
    limbs.get(word).map_or(0, |value| (value >> bit) & 1)
}

fn limbs_ge(a: &[u64; 4], b: &[u64; 4]) -> bool {
    for index in (0..4).rev() {
        if a[index] != b[index] {
            return a[index] > b[index];
        }
    }
    true
}

fn limbs_sub(a: &mut [u64; 4], b: &[u64; 4]) {
    let mut borrow = 0u64;
    for index in 0..4 {
        let (difference, first) = a[index].overflowing_sub(b[index]);
        let (difference, second) = difference.overflowing_sub(borrow);
        a[index] = difference;
        borrow = u64::from(first || second);
    }
}

/// The exact-in-u128 add/sub engine for binary128 (113-bit significand + 3 guard bits).
fn add_bits(a: u128, b: u128, subtract: bool) -> u128 {
    let a_class = classify(a);
    let b_class = classify(b);
    if a_class == FP_NAN || b_class == FP_NAN {
        return ((EXPONENT_MASK as u128) << EXPONENT_SHIFT) | QUIET;
    }
    let b_sign = (b & SIGN != 0) ^ subtract;
    if a_class == FP_INFINITE || b_class == FP_INFINITE {
        if a_class == FP_INFINITE && b_class == FP_INFINITE {
            return if (a & SIGN != 0) != b_sign {
                ((EXPONENT_MASK as u128) << EXPONENT_SHIFT) | QUIET
            } else {
                a
            };
        }
        if a_class == FP_INFINITE {
            return a;
        }
        return if b_sign { b ^ SIGN } else { b };
    }
    if a_class == FP_ZERO && b_class == FP_ZERO {
        let negative = (a & SIGN != 0) == b_sign;
        return if negative { SIGN } else { 0 };
    }
    if b_class == FP_ZERO {
        return a;
    }
    if a_class == FP_ZERO {
        return if subtract { b ^ SIGN } else { b };
    }
    let (mut large_exp, mut large_sig) = unpack(a);
    let (mut small_exp, mut small_sig) = unpack(b);
    let mut small_neg = b_sign;
    let mut large_neg = a & SIGN != 0;
    if (large_exp, large_sig) < (small_exp, small_sig) {
        core::mem::swap(&mut large_exp, &mut small_exp);
        core::mem::swap(&mut large_sig, &mut small_sig);
        core::mem::swap(&mut large_neg, &mut small_neg);
    }
    let difference = (large_exp - small_exp) as u32;
    let mut scaled = small_sig << 3;
    let mut sticky = false;
    if difference > 0 {
        if difference >= 116 {
            sticky = small_sig != 0;
            scaled = 0;
        } else {
            sticky = small_sig & ((1u128 << difference) - 1) != 0;
            scaled >>= difference;
        }
    }
    let mut significand = large_sig << 3;
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
    // `unpack` carries the biased-exponent difference; the significand is scaled by 112 bits.
    pack_binary128(negative, large_exp - 112 - 3, significand)
}

/// Unpack a finite non-zero value as `(unbiased exponent, 113-bit significand)`.
fn unpack(bits: u128) -> (i32, u128) {
    let field = ((bits >> EXPONENT_SHIFT) & EXPONENT_MASK as u128) as i32;
    let stored = bits & SIGNIFICAND_MASK;
    let mut significand = stored | HIDDEN;
    let mut exponent = field - BIAS;
    while significand & HIDDEN == 0 {
        significand <<= 1;
        exponent -= 1;
    }
    (exponent, significand)
}

/// Round `significand * 2^(exponent - 112)` to the 113-bit binary128 significand.
fn pack_binary128(negative: bool, exponent: i32, significand: u128) -> u128 {
    if significand == 0 {
        return if negative { SIGN } else { 0 };
    }
    let highest = 127 - significand.leading_zeros();
    let mut exponent = exponent;
    let (kept, guard, round, rest, sticky_low) = if highest > 112 {
        let drop = highest - 112;
        let guard = (significand >> (drop - 1)) & 1 != 0;
        let round = drop >= 2 && (significand >> (drop - 2)) & 1 != 0;
        let rest = if drop >= 2 {
            significand & ((1u128 << (drop - 2)) - 1) != 0
        } else {
            false
        };
        let kept = significand >> drop;
        exponent += drop as i32;
        (kept, guard, round, rest, kept & 1 != 0)
    } else {
        let shift = (112 - highest) as i32;
        ((significand << shift), false, false, false, false)
    };
    let mut mantissa = kept;
    if guard && (round || rest || sticky_low) {
        mantissa += 1;
        if mantissa >> 113 != 0 {
            mantissa >>= 1;
            exponent += 1;
        }
    }
    let field = exponent + 112 + BIAS;
    let sign = if negative { SIGN } else { 0 };
    if field >= EXPONENT_MASK as i32 {
        return sign | (EXPONENT_MASK as u128) << EXPONENT_SHIFT;
    }
    if field <= 0 {
        // Subnormal: shift the 113-bit significand right by the missing exponent, with rounding.
        let shift = (1 - field) as u32;
        if shift >= 113 {
            return sign;
        }
        let lost = mantissa & ((1u128 << shift) - 1);
        let half = 1u128 << (shift - 1);
        let mut sub = mantissa >> shift;
        if lost > half || (lost == half && (sub & 1) != 0) {
            sub += 1;
        }
        return sign | (sub & SIGNIFICAND_MASK);
    }
    sign | (((field as u128) & EXPONENT_MASK as u128) << EXPONENT_SHIFT)
        | (mantissa & SIGNIFICAND_MASK)
}

/// `x * y`, correctly rounded.
///
/// The 113x113-bit product needs 226 bits, so it is formed in a four-limb accumulator (three
/// 57/56-bit partial products), reduced to the 113-bit significand with three guard bits plus a
/// sticky bit, and rounded by the shared packer.
pub fn mul128_bits(a: u128, b: u128) -> u128 {
    let a_class = classify(a);
    let b_class = classify(b);
    let negative = (a & SIGN != 0) ^ (b & SIGN != 0);
    if a_class == FP_NAN || b_class == FP_NAN {
        return nan_bits();
    }
    if a_class == FP_INFINITE || b_class == FP_INFINITE {
        if a_class == FP_ZERO || b_class == FP_ZERO {
            return nan_bits();
        }
        return if negative {
            SIGN | (EXPONENT_MASK as u128) << EXPONENT_SHIFT
        } else {
            (EXPONENT_MASK as u128) << EXPONENT_SHIFT
        };
    }
    if a_class == FP_ZERO || b_class == FP_ZERO {
        return if negative { SIGN } else { 0 };
    }
    let (a_exp, a_sig) = unpack(a);
    let (b_exp, b_sig) = unpack(b);
    const LOW_MASK: u128 = (1u128 << 56) - 1;
    let ah = a_sig >> 56;
    let al = a_sig & LOW_MASK;
    let bh = b_sig >> 56;
    let bl = b_sig & LOW_MASK;
    let mut limbs = [0u64; 4];
    add_shifted(&mut limbs, al * bl, 0);
    add_shifted(&mut limbs, ah * bl + bh * al, 56);
    add_shifted(&mut limbs, ah * bh, 112);
    // Reduce to 116 bits (113 + 3 guard) with a sticky bit.
    let highest = limb_highest(&limbs).expect("a non-zero product");
    let keep = 116u32;
    let drop = highest + 1 - keep;
    let significand = shift_right_sticky(&limbs, drop);
    // value = significand * 2^(a_exp + b_exp - 224 + drop)
    pack_binary128(negative, a_exp + b_exp - 224 + drop as i32, significand)
}

fn nan_bits() -> u128 {
    (EXPONENT_MASK as u128) << EXPONENT_SHIFT | QUIET
}

/// Add `value << shift` to the little-endian limb accumulator.
fn add_shifted(limbs: &mut [u64; 4], value: u128, shift: u32) {
    let low = value as u64;
    let high = (value >> 64) as u64;
    let start = (shift / 64) as usize;
    let bit = shift % 64;
    let w0 = low << bit;
    let (w1, w2) = if bit == 0 {
        (high, 0)
    } else {
        let shifted = (high as u128) << bit;
        (
            (shifted as u64) | (low >> (64 - bit)),
            (shifted >> 64) as u64,
        )
    };
    add_word(limbs, start, w0);
    add_word(limbs, start + 1, w1);
    add_word(limbs, start + 2, w2);
}

fn add_word(limbs: &mut [u64; 4], index: usize, word: u64) {
    let mut carry = word as u128;
    let mut position = index;
    while carry != 0 && position < 4 {
        let sum = limbs[position] as u128 + carry;
        limbs[position] = sum as u64;
        carry = sum >> 64;
        position += 1;
    }
}

fn limb_highest(limbs: &[u64; 4]) -> Option<u32> {
    for index in (0..4).rev() {
        if limbs[index] != 0 {
            return Some(index as u32 * 64 + (63 - limbs[index].leading_zeros()));
        }
    }
    None
}

/// The top bits above `drop`, with any shifted-out bit folded into bit 0.
fn shift_right_sticky(limbs: &[u64; 4], drop: u32) -> u128 {
    let base = (drop / 64) as usize;
    let bit = drop % 64;
    let mut value = 0u128;
    for offset in 0..3 {
        let index = base + offset;
        if index >= 4 {
            break;
        }
        let word = limbs[index];
        let part = if bit == 0 {
            word
        } else {
            (word >> bit) | limbs.get(index + 1).copied().unwrap_or(0) << (64 - bit)
        };
        if offset * 64 < 128 {
            value |= (part as u128) << (offset * 64);
        } else {
            // Bits above the 128-bit accumulator can only be sticky.
            value |= u128::from(part != 0);
        }
    }
    let mut sticky = false;
    for word in limbs.iter().take(base.min(4)) {
        sticky |= *word != 0;
    }
    if bit != 0 && base < 4 {
        sticky |= (limbs[base] & ((1u64 << bit) - 1)) != 0;
    }
    if sticky {
        value |= 1;
    }
    value
}

#[cfg(test)]
#[path = "binary128_ut.rs"]
mod tests;
