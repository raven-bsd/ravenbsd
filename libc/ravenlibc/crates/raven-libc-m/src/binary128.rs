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

    /// Bridge from C42's `raven_libc_dtoa::LongDouble` byte image.
    ///
    /// The gdtoa output is already the target `long double` bit image; on aarch64/riscv64 that
    /// image is IEEE binary128, so the bridge is the little-endian 128-bit pattern.
    pub fn from_dtoa_bytes(bytes: [u8; 16]) -> Self {
        Self::from_bits(u128::from_le_bytes(bytes))
    }

    /// Bridge to C42's `raven_libc_dtoa::LongDouble` byte image.
    pub fn to_dtoa_bytes(self) -> [u8; 16] {
        self.to_bits().to_le_bytes()
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

    /// `cbrt`: the real cube root, refined by Newton's method.
    ///
    /// The seed is an `f64` cube root of the mantissa (exact `f64` arithmetic, no platform
    /// `cbrt`), scaled by the exponent reduced modulo three; three binary128 Newton steps then
    /// take it to the 113-bit format. The result is within 1 ULP (see the tests and the C47o
    /// report for the evidence); no in-tree binary128 oracle exists for a tighter claim.
    pub fn cbrt(self) -> Self {
        if self.is_zero() || !self.is_finite() {
            return self;
        }
        if self.sign_bit() {
            return self.abs().cbrt().copysign(LongDouble128::zero(true));
        }
        let (exponent, _) = unpack(self.bits);
        // value = m * 2^e with m in [1, 2); reduce e = 3q + r so cbrt = cbrt(m * 2^r) * 2^q.
        let q = exponent.div_euclid(3);
        let scaled = self.scalbn(-(q * 3));
        // `scaled` is now in [1, 8). An f64 Newton on the mantissa gives a ~50-bit seed.
        let seed = scaled.to_f64();
        let mut y = seed;
        let mut step = 0u32;
        while step < 6 {
            y = (2.0 * y + seed / (y * y)) / 3.0;
            step = step.saturating_add(1);
        }
        let two = LongDouble128::from_f64(2.0);
        let three = LongDouble128::from_f64(3.0);
        let mut root = LongDouble128::from_f64(y);
        // Three binary128 Newton steps: 50 -> 100 -> 200 bits of accuracy.
        let mut iteration = 0u32;
        while iteration < 3 {
            root = two.mul(root).add(scaled.div(root.mul(root))).div(three);
            iteration = iteration.saturating_add(1);
        }
        if q == 0 {
            return root;
        }
        let mut result = root.scalbn(q);
        // Rounding of the intermediate steps can land one ULP off; correct with one nudge.
        let mut nudge = 0u32;
        while nudge < 2
            && result.mul(result).mul(result).total_cmp(self) == core::cmp::Ordering::Greater
        {
            result = result.next_after(LongDouble128::zero(false));
            nudge = nudge.saturating_add(1);
        }
        result
    }

    /// `hypot`: `sqrt(x^2 + y^2)` without intermediate overflow or underflow.
    pub fn hypot(self, other: Self) -> Self {
        if self.is_infinite() || other.is_infinite() {
            return LongDouble128::infinity(false);
        }
        if self.is_nan() || other.is_nan() {
            return LongDouble128::nan();
        }
        let a = self.abs();
        let b = other.abs();
        let (large, small) = if a.total_cmp(b) == core::cmp::Ordering::Less {
            (b, a)
        } else {
            (a, b)
        };
        if large.is_zero() {
            return LongDouble128::zero(false);
        }
        let ratio = small.div(large);
        let sum = LongDouble128::one().add(ratio.mul(ratio));
        large.mul(sum.sqrt())
    }

    /// `|x|`.
    pub fn abs(self) -> Self {
        Self {
            bits: self.bits & !SIGN,
        }
    }

    /// `e^x`.
    ///
    /// Range reduction: `x = k*ln2 + r` with `|r| <= ln2/2` and integer `k`; the reduction uses
    /// the split constant `LN2_HI + LN2_LO` so `r` keeps its bits for large `|x|`, and `exp(r)`
    /// is a 27-term Taylor series (the first omitted term is below `2^-118` for `|r| <= 0.35`).
    /// The result is then scaled by `2^k` with the correctly-rounded `scalbn`.
    #[allow(clippy::should_implement_trait)] // the C `expl` body; there is no trait here
    pub fn exp(self) -> Self {
        if self.is_nan() {
            return self;
        }
        if self.is_infinite() {
            return if self.sign_bit() {
                Self::zero(false)
            } else {
                self
            };
        }
        if self.is_zero() {
            return Self::one();
        }
        let ln2 = Self::ln2_hi().add(Self::ln2_lo());
        let k = self.div(ln2).round();
        let Some(k) = k.to_i64() else {
            return if self.sign_bit() {
                Self::zero(false)
            } else {
                Self::infinity(false)
            };
        };
        if k > 16400 {
            return Self::infinity(false);
        }
        if k < -16500 {
            return Self::zero(false);
        }
        let kf = Self::from_i64(k);
        // r = x - k*ln2 with the 40-bit parts (exact products) and the residual; the series
        // stays accurate even if the rounded quotient puts |r| a little above ln2/2.
        let r = self
            .sub(kf.mul(Self::ln2_1()))
            .sub(kf.mul(Self::ln2_2()))
            .sub(kf.mul(Self::ln2_3()));
        let scale = i32::try_from(k).unwrap_or(0);
        Self::exp_series(r).scalbn(scale)
    }

    /// The natural logarithm.
    ///
    /// `x = m * 2^k` with `m` in `[1/sqrt2, sqrt2)` (exponent from `frexp`, one extra shift when
    /// the mantissa exceeds `sqrt2`), then `ln x = k*ln2 + 2*atanh((m-1)/(m+1))`. The `atanh`
    /// series is truncated after 26 odd terms, whose first omitted term is below `2^-125` for
    /// `|s| <= 0.172`; `k*ln2` uses the split constant.
    #[allow(clippy::should_implement_trait)]
    pub fn ln(self) -> Self {
        if self.is_nan() {
            return self;
        }
        if self.is_zero() {
            return Self::infinity(true);
        }
        if self.sign_bit() {
            return Self::nan();
        }
        if self.is_infinite() {
            return self;
        }
        if self.bits == Self::one().bits {
            return Self::zero(false);
        }
        let (mantissa, exponent) = self.frexp();
        // frexp gives [0.5, 1); shift to [1, 2).
        let mut m = mantissa.scalbn(1);
        let mut e = exponent.saturating_sub(1);
        let sqrt2 = Self::from_f64(2.0).sqrt();
        if m.total_cmp(sqrt2) == core::cmp::Ordering::Greater {
            m = m.scalbn(-1);
            e = e.saturating_add(1);
        }
        let s = m.sub(Self::one()).div(m.add(Self::one()));
        let series = Self::atanh_series(s);
        let ef = Self::from_i64(i64::from(e));
        ef.mul(Self::ln2_hi())
            .add(ef.mul(Self::ln2_lo()))
            .add(series)
    }

    /// `2^x`: the integer part becomes an exact power-of-two scale and the fraction goes through
    /// the `exp` series with `r*ln2`.
    pub fn exp2(self) -> Self {
        if self.is_nan() {
            return self;
        }
        if self.is_infinite() {
            return if self.sign_bit() {
                Self::zero(false)
            } else {
                self
            };
        }
        if self.is_zero() {
            return Self::one();
        }
        let k = self.round();
        let Some(k) = k.to_i64() else {
            return if self.sign_bit() {
                Self::zero(false)
            } else {
                Self::infinity(false)
            };
        };
        if k > 16400 {
            return Self::infinity(false);
        }
        if k < -16500 {
            return Self::zero(false);
        }
        let kf = Self::from_i64(k);
        let r = self.sub(kf);
        // r*ln2 with the split constant; |r| <= 0.5 so the product keeps its accuracy.
        let argument = r.mul(Self::ln2_hi()).add(r.mul(Self::ln2_lo()));
        let scale = i32::try_from(k).unwrap_or(0);
        Self::exp_series(argument).scalbn(scale)
    }

    /// The base-2 logarithm; an exact power of two returns its integer exponent exactly.
    pub fn log2(self) -> Self {
        if !self.is_finite() || self.is_zero() || self.sign_bit() {
            return self.ln().div(Self::ln2_hi());
        }
        let (mantissa, exponent) = self.frexp();
        if mantissa.bits == Self::from_f64(0.5).bits {
            // x = 2^(exponent - 1) exactly.
            return Self::from_i64(i64::from(exponent.saturating_sub(1)));
        }
        self.ln().div(Self::ln2_hi())
    }

    /// The base-10 logarithm.
    pub fn log10(self) -> Self {
        self.ln().div(Self::ln10())
    }

    /// `ln 2`, rounded to binary128 with its residual split so `LN2_HI + LN2_LO` is ln 2 to
    /// ~2^-231. Computed with 200-digit decimal arithmetic (see the C47p report) and
    /// round-to-nearest-even.
    pub const fn ln2_hi() -> Self {
        Self {
            bits: 0x3ffe_62e4_2fef_a39e_f357_93c7_6730_07e6,
        }
    }

    /// The residual `ln 2 - LN2_HI`.
    pub const fn ln2_lo() -> Self {
        Self {
            bits: 0xbf8a_2a17_e197_9b31_ace9_3a4e_be5d_148f,
        }
    }

    /// The first 40-bit part of `ln 2`; `k*LN2_1` is exact for every `|k| < 2^73`, which is
    /// what keeps `expl`'s range reduction accurate for large arguments.
    pub const fn ln2_1() -> Self {
        Self {
            bits: 0x3ffe_62e4_2fef_a400_0000_0000_0000_0000,
        }
    }

    /// The second 40-bit part of `ln 2`.
    pub const fn ln2_2() -> Self {
        Self {
            bits: 0xbfd4_8432_a1b0_e200_0000_0000_0000_0000,
        }
    }

    /// The residual `ln 2 - LN2_1 - LN2_2`.
    pub const fn ln2_3() -> Self {
        Self {
            bits: 0xbfaa_8cff_81a1_2a17_e197_9b31_ace9_3a4f,
        }
    }

    /// `ln 10`, rounded to binary128 (0.5 ULP).
    pub const fn ln10() -> Self {
        Self {
            bits: 0x4000_26bb_1bbb_5551_582d_d4ad_ac57_05a6,
        }
    }

    /// `pi/2`, split into seven 50-bit parts for the trigonometric reduction.
    ///
    /// Every part keeps at most 50 significant bits, so `n*part` is exact for every `|n| < 2^63`
    /// (the whole quadrant-index range): 50 + 63 = 113 bits. The seven parts sum to `pi/2` with
    /// an absolute residual below `2^-360` (500-digit decimal pi, round-to-nearest-even), so the
    /// reduction keeps its relative accuracy next to the zeros of `sin`/`cos` as well.
    pub const fn pio2_1() -> Self {
        Self {
            bits: 0x3fff_921f_b544_42d1_8000_0000_0000_0000,
        }
    }

    /// The second 50-bit part of `pi/2`.
    pub const fn pio2_2() -> Self {
        Self {
            bits: 0x3fc9_1a62_6331_45c0_8000_0000_0000_0000,
        }
    }

    /// The third 50-bit part of `pi/2`.
    pub const fn pio2_3() -> Self {
        Self {
            bits: 0xbf95_1f19_76b7_ed8f_8000_0000_0000_0000,
        }
    }

    /// The fourth 50-bit part of `pi/2`.
    pub const fn pio2_4() -> Self {
        Self {
            bits: 0xbf62_dd66_0ce2_ff7d_0000_0000_0000_0000,
        }
    }

    /// The fifth 50-bit part of `pi/2`.
    pub const fn pio2_5() -> Self {
        Self {
            bits: 0xbf2e_0567_13b1_9376_8000_0000_0000_0000,
        }
    }

    /// The sixth 50-bit part of `pi/2`.
    pub const fn pio2_6() -> Self {
        Self {
            bits: 0xbefb_d6be_f0ce_397f_8000_0000_0000_0000,
        }
    }

    /// The seventh 50-bit part of `pi/2` (what remains is below `2^-360`).
    pub const fn pio2_7() -> Self {
        Self {
            bits: 0x3ec7_bbdf_2a33_679a_8000_0000_0000_0000,
        }
    }

    /// `pi/2` rounded to binary128.
    pub const fn pio2_hi() -> Self {
        Self {
            bits: 0x3fff_921f_b544_42d1_8469_898c_c517_01b8,
        }
    }

    /// The residual `pi/2 - PIO2_HI`.
    pub const fn pio2_lo() -> Self {
        Self {
            bits: 0x3f8c_cd12_9024_e088_a67c_c740_20bb_ea64,
        }
    }

    /// `pi` rounded to binary128.
    pub const fn pi_hi() -> Self {
        Self {
            bits: 0x4000_921f_b544_42d1_8469_898c_c517_01b8,
        }
    }

    /// The residual `pi - PI_HI`.
    pub const fn pi_lo() -> Self {
        Self {
            bits: 0x3f8d_cd12_9024_e088_a67c_c740_20bb_ea64,
        }
    }

    /// `sin x`, in radians.
    ///
    /// The reduction is `n = round(x/(pi/2))` and `r = x - n*(PIO2_1 + ... + PIO2_7)`, with
    /// `|r| <= pi/4`. Every part keeps 50 significant bits, so each `n*PIO2_i` is exact for the
    /// whole `|n| < 2^63` index range, and each subtraction is a Sterbenz cancellation; the
    /// seven parts carry `pi/2` to `2^-360`, so `r` keeps the input's bits even next to a zero of
    /// `sin` at a large quadrant index. The series truncates after `r^33/33!`, whose first
    /// omitted term is below `2^-128` for `|r| <= pi/4`.
    ///
    /// Accuracy: at most 4 ULP (measured maxima: `sin` 3, `cos` 3, `tan` 4) for every `|x|`
    /// whose quadrant index fits an `i64` (`|x| < 2^63 * pi/2`, about `1.45e19`). Beyond that,
    /// and for `+-inf`, `sin`/`cos`/`tan` return NaN: no Payne-Hanek reduction is implemented.
    pub fn sin(self) -> Self {
        let (sine, _) = self.sincos();
        sine
    }

    /// `cos x`, in radians; the same reduction and bound as [`Self::sin`].
    pub fn cos(self) -> Self {
        let (_, cosine) = self.sincos();
        cosine
    }

    /// `sin x` and `cos x` from one reduction.
    pub fn sincos(self) -> (Self, Self) {
        if self.is_nan() || self.is_infinite() {
            return (Self::nan(), Self::nan());
        }
        if self.is_zero() {
            return (self, Self::one());
        }
        let Some((quadrant, r)) = self.reduce_half_pi() else {
            return (Self::nan(), Self::nan());
        };
        let sine = Self::sin_series(r);
        let cosine = Self::cos_series(r);
        match quadrant {
            0 => (sine, cosine),
            1 => (cosine, Self::zero(false).sub(sine)),
            2 => (Self::zero(false).sub(sine), Self::zero(false).sub(cosine)),
            _ => (Self::zero(false).sub(cosine), sine),
        }
    }

    /// `tan x = sin x / cos x`; accurate wherever `cos` is not zero, which the `pi/4` bound
    /// on the reduced argument guarantees cannot happen exactly.
    pub fn tan(self) -> Self {
        let (sine, cosine) = self.sincos();
        sine.div(cosine)
    }

    /// `atan x`.
    ///
    /// `|x| > 1` uses `pi/2 - atan(1/x)`. The remaining `|v| <= 1` is halved three times with
    /// `atan(v) = 2 atan(v/(1 + sqrt(1 + v^2)))`, which brings `|v|` below `tan(pi/32)`, and
    /// the alternating Taylor series runs to `v^61/61` (first omitted term below `2^-200`).
    /// Values at or below `2^-4` skip the halving (the series converges there without help).
    pub fn atan(self) -> Self {
        if self.is_nan() {
            return self;
        }
        if self.is_zero() {
            return self;
        }
        if self.is_infinite() {
            return Self::pio2_hi().add(Self::pio2_lo()).copysign(self);
        }
        let negative = self.sign_bit();
        let magnitude = self.abs();
        let (value, reciprocal) =
            if magnitude.total_cmp(Self::one()) == core::cmp::Ordering::Greater {
                (Self::one().div(magnitude), true)
            } else {
                (magnitude, false)
            };
        let angle = Self::atan_kernel(value);
        let result = if reciprocal {
            Self::pio2_hi().add(Self::pio2_lo()).sub(angle)
        } else {
            angle
        };
        if negative {
            Self::zero(false).sub(result)
        } else {
            result
        }
    }

    /// `atan2(y, x)` with `self = y` and the quadrant from the signs of both arguments.
    pub fn atan2(self, x: Self) -> Self {
        if self.is_nan() || x.is_nan() {
            return Self::nan();
        }
        let pi_value = Self::pi_hi().add(Self::pi_lo());
        if self.is_zero() {
            return if x.sign_bit() {
                pi_value.copysign(self)
            } else {
                self
            };
        }
        if x.is_zero() {
            return Self::pio2_hi().add(Self::pio2_lo()).copysign(self);
        }
        if self.is_infinite() && x.is_infinite() {
            let magnitude = if x.sign_bit() {
                pi_value.mul(Self::from_f64(0.75))
            } else {
                pi_value.mul(Self::from_f64(0.25))
            };
            return magnitude.copysign(self);
        }
        let base = self.div(x).atan();
        if x.sign_bit() {
            if self.sign_bit() {
                base.sub(pi_value)
            } else {
                base.add(pi_value)
            }
        } else {
            base
        }
    }

    /// `asin x` for `|x| <= 1`, as `atan2(x, sqrt((1-x)*(1+x)))`.
    pub fn asin(self) -> Self {
        if self.is_nan() {
            return self;
        }
        let magnitude = self.abs();
        if magnitude.total_cmp(Self::one()) == core::cmp::Ordering::Greater {
            return Self::nan();
        }
        if magnitude.bits == Self::one().bits {
            return Self::pio2_hi().add(Self::pio2_lo()).copysign(self);
        }
        if self.is_zero() {
            return self;
        }
        let root = Self::one().sub(self).mul(Self::one().add(self)).sqrt();
        self.atan2(root)
    }

    /// `acos x` for `|x| <= 1`, as `atan2(sqrt((1-x)*(1+x)), x)`; the ratio form keeps the
    /// relative accuracy next to `x = 1` that `pi/2 - asin x` would lose.
    pub fn acos(self) -> Self {
        if self.is_nan() {
            return self;
        }
        let magnitude = self.abs();
        if magnitude.total_cmp(Self::one()) == core::cmp::Ordering::Greater {
            return Self::nan();
        }
        if magnitude.bits == Self::one().bits {
            return if self.sign_bit() {
                Self::pi_hi().add(Self::pi_lo())
            } else {
                Self::zero(false)
            };
        }
        let root = Self::one().sub(self).mul(Self::one().add(self)).sqrt();
        root.atan2(self)
    }

    /// `sinh x`.
    ///
    /// `|x| < 1` uses the Maclaurin series (the first omitted term is below `2^-160` at
    /// `|x| = 1`). Above that, `exp(x - ln2) - exp(-x - ln2)` halves the argument inside
    /// `exp`, so both the accuracy and the overflow threshold are `sinh`'s own; the
    /// subtraction `x - ln2` is carried as an exact two-sum pair and re-applied through
    /// `exp(hi)*(1 + lo)`, because a plain subtraction would round at `ulp(x)` and lose
    /// `log2(x)` bits of `ln 2`.
    pub fn sinh(self) -> Self {
        if self.is_nan() || self.is_infinite() || self.is_zero() {
            return self;
        }
        let negative = self.sign_bit();
        let magnitude = self.abs();
        let value = if magnitude.total_cmp(Self::one()) == core::cmp::Ordering::Less {
            Self::sinh_series(magnitude)
        } else {
            let half_exp = Self::exp_shifted(magnitude);
            let ln2 = Self::ln2_hi().add(Self::ln2_lo());
            half_exp.sub(Self::zero(false).sub(magnitude).sub(ln2).exp())
        };
        if negative {
            Self::zero(false).sub(value)
        } else {
            value
        }
    }

    /// `cosh x`; the same series and shifted-exponential split as [`Self::sinh`].
    pub fn cosh(self) -> Self {
        if self.is_nan() {
            return self;
        }
        if self.is_infinite() {
            return Self::infinity(false);
        }
        if self.is_zero() {
            return Self::one();
        }
        let magnitude = self.abs();
        if magnitude.total_cmp(Self::one()) == core::cmp::Ordering::Less {
            Self::cosh_series(magnitude)
        } else {
            let half_exp = Self::exp_shifted(magnitude);
            let ln2 = Self::ln2_hi().add(Self::ln2_lo());
            half_exp.add(Self::zero(false).sub(magnitude).sub(ln2).exp())
        }
    }

    /// `tanh x`: the series ratio below 1 and `1 - 2/(exp(2x) + 1)` above it. The subtraction
    /// form avoids the cancellation in `(exp(2x) - 1)/(exp(2x) + 1)` for large `x`.
    pub fn tanh(self) -> Self {
        if self.is_nan() {
            return self;
        }
        if self.is_infinite() {
            return Self::one().copysign(self);
        }
        if self.is_zero() {
            return self;
        }
        let negative = self.sign_bit();
        let magnitude = self.abs();
        let value = if magnitude.total_cmp(Self::one()) == core::cmp::Ordering::Less {
            Self::sinh_series(magnitude).div(Self::cosh_series(magnitude))
        } else {
            let exponential = magnitude.add(magnitude).exp();
            Self::one().sub(Self::from_f64(2.0).div(exponential.add(Self::one())))
        };
        if negative {
            Self::zero(false).sub(value)
        } else {
            value
        }
    }

    /// `asinh x`.
    ///
    /// `|x| <= 0.5` uses the Maclaurin series (64 terms; the first omitted term is below
    /// `2^-128`), `|x| >= 2^8000` uses `ln(2x)` (the correction is below `2^-16000`), and the
    /// rest uses `ln(x + sqrt(x^2 + 1))`.
    pub fn asinh(self) -> Self {
        if self.is_nan() || self.is_infinite() || self.is_zero() {
            return self;
        }
        let negative = self.sign_bit();
        let magnitude = self.abs();
        let value = if magnitude.total_cmp(Self::from_f64(0.5)) != core::cmp::Ordering::Greater {
            Self::asinh_series(magnitude)
        } else if magnitude.ilogb() > 8000 {
            magnitude.add(magnitude).ln()
        } else {
            magnitude
                .add(magnitude.mul(magnitude).add(Self::one()).sqrt())
                .ln()
        };
        if negative {
            Self::zero(false).sub(value)
        } else {
            value
        }
    }

    /// `acosh x`, as `2 ln(sqrt((x+1)/2) + sqrt((x-1)/2))`; the half-angle form keeps the
    /// accuracy next to `x = 1` that `ln(x + sqrt(x^2 - 1))` would lose.
    pub fn acosh(self) -> Self {
        if self.is_nan() {
            return self;
        }
        if self.total_cmp(Self::one()) == core::cmp::Ordering::Less {
            return Self::nan();
        }
        if self.bits == Self::one().bits {
            return Self::zero(false);
        }
        if self.is_infinite() {
            return self;
        }
        let half = Self::from_f64(0.5);
        let lower = self.sub(Self::one()).mul(half).sqrt();
        let upper = self.add(Self::one()).mul(half).sqrt();
        lower.add(upper).ln().mul(Self::from_f64(2.0))
    }

    /// `atanh x`.
    ///
    /// `|x| <= 0.125` uses the `atanh` series (26 odd terms, first omitted below `2^-158`);
    /// above that, `0.5 ln((1+x)/(1-x))`.
    pub fn atanh(self) -> Self {
        if self.is_nan() {
            return self;
        }
        let magnitude = self.abs();
        match magnitude.total_cmp(Self::one()) {
            core::cmp::Ordering::Greater => return Self::nan(),
            core::cmp::Ordering::Equal => {
                return Self::infinity(true).copysign(self);
            }
            core::cmp::Ordering::Less => {}
        }
        if self.is_zero() {
            return self;
        }
        let value = if magnitude.total_cmp(Self::from_f64(0.125)) != core::cmp::Ordering::Greater {
            Self::atanh_series(magnitude).mul(Self::from_f64(0.5))
        } else {
            Self::one()
                .add(magnitude)
                .div(Self::one().sub(magnitude))
                .ln()
                .mul(Self::from_f64(0.5))
        };
        if self.sign_bit() {
            Self::zero(false).sub(value)
        } else {
            value
        }
    }

    /// `(quadrant, r)` with `x = quadrant*(pi/2) + r` and `|r| <= pi/4`, or `None` when the
    /// quadrant index does not fit an `i64`.
    fn reduce_half_pi(self) -> Option<(i64, Self)> {
        let quotient = self.div(Self::pio2_hi()).round();
        let n = quotient.to_i64()?;
        let nf = Self::from_i64(n);
        let r = self
            .sub(nf.mul(Self::pio2_1()))
            .sub(nf.mul(Self::pio2_2()))
            .sub(nf.mul(Self::pio2_3()))
            .sub(nf.mul(Self::pio2_4()))
            .sub(nf.mul(Self::pio2_5()))
            .sub(nf.mul(Self::pio2_6()))
            .sub(nf.mul(Self::pio2_7()));
        Some((n & 3, r))
    }

    /// `atan` on `|v| <= 1`; the halving only runs when the series would need more terms.
    fn atan_kernel(value: Self) -> Self {
        let mut v = value;
        let mut scale = 1i32;
        if v.total_cmp(Self::from_f64(0.0625)) == core::cmp::Ordering::Greater {
            for _ in 0..3 {
                v = v.div(Self::one().add(Self::one().add(v.mul(v)).sqrt()));
                scale *= 2;
            }
        }
        let v2 = v.mul(v);
        let mut term = v;
        let mut sum = v;
        let mut index: i64 = 3;
        while index <= 61 {
            term = term.mul(v2);
            let contribution = term.div(Self::from_i64(index));
            sum = if (index / 2) % 2 == 1 {
                sum.sub(contribution)
            } else {
                sum.add(contribution)
            };
            index += 2;
        }
        let mut result = sum;
        let mut step = 1i32;
        while step < scale {
            result = result.add(result);
            step *= 2;
        }
        result
    }

    /// `sum r^(2n+1)/(2n+1)!` up to `n = 16`; the first omitted term is below `2^-128` at
    /// `|r| = pi/4`.
    fn sin_series(r: Self) -> Self {
        let r2 = r.mul(r);
        let mut term = r;
        let mut sum = r;
        let mut index: i64 = 3;
        while index <= 33 {
            term = term.mul(r2).div(Self::from_i64(index * (index - 1)));
            sum = if (index / 2) % 2 == 1 {
                sum.sub(term)
            } else {
                sum.add(term)
            };
            index += 2;
        }
        sum
    }

    /// `sum r^(2n)/(2n)!` up to `n = 17`.
    fn cos_series(r: Self) -> Self {
        let r2 = r.mul(r);
        let mut term = Self::one();
        let mut sum = Self::one();
        let mut index: i64 = 2;
        while index <= 34 {
            term = term.mul(r2).div(Self::from_i64(index * (index - 1)));
            sum = if (index / 2) % 2 == 1 {
                sum.sub(term)
            } else {
                sum.add(term)
            };
            index += 2;
        }
        sum
    }

    /// `sum x^(2n+1)/(2n+1)!` up to `n = 22`.
    fn sinh_series(x: Self) -> Self {
        let x2 = x.mul(x);
        let mut term = x;
        let mut sum = x;
        let mut n: i64 = 1;
        while n <= 22 {
            term = term.mul(x2).div(Self::from_i64((2 * n) * (2 * n + 1)));
            sum = sum.add(term);
            n += 1;
        }
        sum
    }

    /// `sum x^(2n)/(2n)!` up to `n = 22`.
    fn cosh_series(x: Self) -> Self {
        let x2 = x.mul(x);
        let mut term = Self::one();
        let mut sum = Self::one();
        let mut n: i64 = 1;
        while n <= 22 {
            term = term.mul(x2).div(Self::from_i64((2 * n - 1) * (2 * n)));
            sum = sum.add(term);
            n += 1;
        }
        sum
    }

    /// `exp(x - ln2)` for `x >= 1`, accurate even though a plain `x - ln2` rounds at
    /// `ulp(x)`: the difference is carried as the exact pair `(shift, tail)` from
    /// [`Self::two_sum`] and the tail is applied as `exp(shift)*(1 + tail)`.
    fn exp_shifted(x: Self) -> Self {
        let (first, first_tail) = Self::two_sum(x, Self::zero(false).sub(Self::ln2_hi()));
        let (shift, tail) = Self::two_sum(first, first_tail.sub(Self::ln2_lo()));
        let exponential = shift.exp();
        exponential.mul(Self::one().add(tail))
    }

    /// Knuth's two-sum: `(s, e)` with `s = round(a + b)` and `s + e = a + b` exactly.
    fn two_sum(a: Self, b: Self) -> (Self, Self) {
        let s = a.add(b);
        let b_virtual = s.sub(a);
        let a_virtual = s.sub(b_virtual);
        let b_round = b.sub(b_virtual);
        let a_round = a.sub(a_virtual);
        (s, a_round.add(b_round))
    }

    /// `x - x^3/6 + 3x^5/40 - ...`, 64 terms; the ratio test keeps the truncation below
    /// `2^-128` for `|x| <= 0.5`.
    fn asinh_series(x: Self) -> Self {
        let x2 = x.mul(x);
        let mut term = x;
        let mut sum = x;
        let mut n: i64 = 1;
        while n <= 64 {
            let odd = 2 * n - 1;
            term = Self::zero(false).sub(
                term.mul(x2)
                    .mul(Self::from_i64(odd * odd))
                    .div(Self::from_i64(2 * n * (2 * n + 1))),
            );
            sum = sum.add(term);
            n += 1;
        }
        sum
    }

    /// `sum_{n=0..30} r^n / n!`, evaluated backwards for stability; the first omitted term is
    /// below `2^-128` for `|r| <= 0.7` (a rounded quotient can leave `|r|` a little above
    /// `ln2/2`).
    fn exp_series(r: Self) -> Self {
        let mut acc = Self::one();
        let mut n: i64 = 30;
        while n >= 1 {
            acc = Self::one().add(acc.mul(r).div(Self::from_i64(n)));
            n = n.saturating_sub(1);
        }
        acc
    }

    /// `2*(s + s^3/3 + s^5/5 + ... + s^51/51)` (26 odd terms).
    fn atanh_series(s: Self) -> Self {
        let s2 = s.mul(s);
        let mut power = s;
        let mut sum = s;
        let mut denominator: i64 = 3;
        while denominator <= 51 {
            power = power.mul(s2);
            sum = sum.add(power.div(Self::from_i64(denominator)));
            denominator = denominator.saturating_add(2);
        }
        sum.add(sum)
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

    /// `frexp`: split into a mantissa in `[0.5, 1)` and a base-2 exponent, `x = m * 2^e`.
    ///
    /// Zero returns `(0, 0)`; an infinity or NaN returns `(x, 0)` unchanged. Subnormals are
    /// normalized first, so `e` is the true exponent of the leading bit.
    pub fn frexp(self) -> (Self, i32) {
        if self.is_zero() || !self.is_finite() {
            return (self, 0);
        }
        let (exponent, significand) = unpack(self.bits);
        let sign = if self.sign_bit() { SIGN } else { 0 };
        // `value = significand * 2^(exponent - 112)` and the leading bit of `significand` is the
        // hidden bit, so the mantissa has the unbiased exponent -1 and `e = exponent + 1`.
        let mantissa = Self {
            bits: sign
                | (((BIAS - 1) as u128) << EXPONENT_SHIFT)
                | (significand & SIGNIFICAND_MASK),
        };
        (mantissa, exponent.saturating_add(1))
    }

    /// `ilogb`: the unbiased base-2 exponent with the C edge results.
    ///
    /// `ilogb(0)` is `FP_ILOGB0`; NaN and infinity return `FP_ILOGBNAN`.
    pub fn ilogb(self) -> i32 {
        if self.is_zero() {
            return crate::consts::FP_ILOGB0;
        }
        if !self.is_finite() {
            return crate::consts::FP_ILOGBNAN;
        }
        let (exponent, _) = unpack(self.bits);
        exponent
    }

    /// `logb`: `ilogb` as a value; zero is `-inf` and infinity keeps its sign.
    pub fn logb(self) -> Self {
        if self.is_nan() {
            return self;
        }
        if self.is_infinite() {
            return Self::infinity(self.sign_bit());
        }
        if self.is_zero() {
            return Self::infinity(true);
        }
        Self::from_i64(i64::from(self.ilogb()))
    }

    /// `significand`: `x` scaled to `[1, 2)`; zero, infinity and NaN pass through.
    pub fn significand(self) -> Self {
        if !self.is_finite() || self.is_zero() {
            return self;
        }
        let (_, significand) = unpack(self.bits);
        let sign = if self.sign_bit() { SIGN } else { 0 };
        Self {
            bits: sign | ((BIAS as u128) << EXPONENT_SHIFT) | (significand & SIGNIFICAND_MASK),
        }
    }

    /// `modf`: `(fractional part, integral part)`, both carrying the sign of `x`.
    ///
    /// Infinity returns `(0, x)` and NaN returns `(x, x)`.
    pub fn modf(self) -> (Self, Self) {
        if self.is_nan() {
            return (self, self);
        }
        if self.is_infinite() {
            return (Self::zero(self.sign_bit()), self);
        }
        let integral = self.trunc();
        (self.sub(integral), integral)
    }

    /// The value scaled to `[1, 2)` (the mantissa used by the logarithm reduction).
    pub fn mantissa(self) -> Self {
        Self {
            bits: ((BIAS as u128) << EXPONENT_SHIFT) | self.significand_bits(),
        }
    }

    /// `floor`.
    pub fn floor(self) -> Self {
        Self {
            bits: integral_bits(self.bits, IntegralMode::Floor),
        }
    }

    /// `ceil`.
    pub fn ceil(self) -> Self {
        Self {
            bits: integral_bits(self.bits, IntegralMode::Ceil),
        }
    }

    /// `trunc`.
    pub fn trunc(self) -> Self {
        Self {
            bits: integral_bits(self.bits, IntegralMode::Trunc),
        }
    }

    /// `roundeven`: ties to even.
    pub fn roundeven(self) -> Self {
        Self {
            bits: integral_bits(self.bits, IntegralMode::Even),
        }
    }

    /// Whether the value is a normal number.
    pub fn is_normal(self) -> bool {
        classify(self.bits) == FP_NORMAL
    }

    /// The IEEE partial order; `None` when either operand is a NaN.
    pub fn partial_cmp(self, other: Self) -> Option<core::cmp::Ordering> {
        if self.is_nan() || other.is_nan() {
            return None;
        }
        Some(self.total_cmp(other))
    }

    /// `x mod y` with the sign of `x` (`fmod`).
    pub fn fmod(self, other: Self) -> Self {
        if self.is_nan() || other.is_nan() || self.is_infinite() || other.is_zero() {
            return Self::nan();
        }
        if other.is_infinite() || self.is_zero() {
            return self;
        }
        self.sub(self.div(other).trunc().mul(other))
    }

    /// `remainder`: `x - n*y` with `n` the nearest even integer to `x/y`.
    pub fn remainder(self, other: Self) -> Self {
        self.remquo(other).0
    }

    /// `remquo`: the remainder and the low quotient bits with the sign.
    pub fn remquo(self, other: Self) -> (Self, i32) {
        if self.is_nan() || other.is_nan() || self.is_infinite() || other.is_zero() {
            return (Self::nan(), 0);
        }
        if other.is_infinite() || self.is_zero() {
            return (self, 0);
        }
        let quotient = self.div(other).roundeven();
        let magnitude = quotient.abs().to_i64().unwrap_or(0) & 0x7f;
        let negative = self.sign_bit() ^ other.sign_bit();
        let low = if negative {
            -(magnitude as i32)
        } else {
            magnitude as i32
        };
        (self.sub(quotient.mul(other)), low)
    }

    /// `fdim`: the positive difference.
    pub fn fdim(self, other: Self) -> Self {
        if self.is_nan() || other.is_nan() {
            return Self::nan();
        }
        if self.total_cmp(other) == core::cmp::Ordering::Greater {
            self.sub(other)
        } else {
            Self::zero(false)
        }
    }

    /// `fmax`: the larger value, with `+0` winning over `-0` and a NaN yielding the other operand.
    pub fn fmax(self, other: Self) -> Self {
        if self.is_nan() {
            return other;
        }
        if other.is_nan() {
            return self;
        }
        if self.total_cmp(other) == core::cmp::Ordering::Greater {
            self
        } else {
            other
        }
    }

    /// `fmin`: the smaller value, with `-0` winning over `+0` and a NaN yielding the other operand.
    pub fn fmin(self, other: Self) -> Self {
        if self.is_nan() {
            return other;
        }
        if other.is_nan() {
            return self;
        }
        if self.total_cmp(other) == core::cmp::Ordering::Less {
            self
        } else {
            other
        }
    }

    /// `nextafter` towards `other`.
    pub fn next_after(self, other: Self) -> Self {
        if self.is_nan() || other.is_nan() {
            return Self::nan();
        }
        let key = order_key(self.bits);
        let target = order_key(other.bits);
        if key == target {
            return other;
        }
        let next = if key < target { key + 1 } else { key - 1 };
        Self {
            bits: if next & SIGN != 0 { next ^ SIGN } else { !next },
        }
    }
}

/// The integral rounding modes.
enum IntegralMode {
    Floor,
    Ceil,
    Trunc,
    Even,
}

/// Round to an integer in the requested mode (the f128 integral family).
fn integral_bits(a: u128, mode: IntegralMode) -> u128 {
    if !matches!(classify(a), FP_ZERO | FP_SUBNORMAL | FP_NORMAL) {
        return a;
    }
    if classify(a) == FP_ZERO {
        return a;
    }
    let negative = a & SIGN != 0;
    let (exponent, significand) = unpack(a);
    if exponent >= 112 {
        return a;
    }
    let sign = if negative { SIGN } else { 0 };
    if exponent < -1 {
        let keep = match mode {
            IntegralMode::Floor => negative,
            IntegralMode::Ceil => !negative,
            IntegralMode::Trunc | IntegralMode::Even => false,
        };
        return if keep {
            sign | (0x3fff_u128 << EXPONENT_SHIFT)
        } else {
            sign
        };
    }
    if exponent == -1 {
        // |x| in [0.5, 1).
        let keep = match mode {
            IntegralMode::Floor => negative,
            IntegralMode::Ceil => !negative,
            IntegralMode::Trunc | IntegralMode::Even => false,
        };
        return if keep {
            sign | (0x3fff_u128 << EXPONENT_SHIFT)
        } else {
            sign
        };
    }
    let drop = (112 - exponent) as u32;
    let kept = (significand >> drop) << drop;
    let lost = significand & ((1u128 << drop) - 1);
    if lost == 0 {
        return a;
    }
    let increment = match mode {
        IntegralMode::Floor => negative,
        IntegralMode::Ceil => !negative,
        IntegralMode::Trunc => false,
        IntegralMode::Even => {
            let half = 1u128 << (drop - 1);
            lost > half || (lost == half && ((kept >> drop) & 1) != 0)
        }
    };
    let magnitude = if increment {
        kept + (1u128 << drop)
    } else {
        kept
    };
    from_parts_bits(negative, exponent, magnitude)
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
        // The exact significand is narrower than 113 bits (a cancellation or a short sum):
        // shift it up to the hidden bit and take the same amount off the scale, so the value
        // `significand * 2^exponent` is preserved.
        let shift = (112 - highest) as i32;
        exponent -= shift;
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
