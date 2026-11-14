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

#[cfg(test)]
#[path = "binary128_ut.rs"]
mod tests;
