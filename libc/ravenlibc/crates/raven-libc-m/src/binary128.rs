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

#[cfg(test)]
#[path = "binary128_ut.rs"]
mod tests;
