// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Integer fixed-point helpers shared by the tone generator and the resampler.
//!
//! The crate must not use floating point (DR-14), so every trigonometric value is produced by
//! [`sin_q32`]: a `const fn` that reduces its argument modulo two pi and evaluates a truncated
//! Taylor series in `i128`. The absolute error is below 1e-6 of full scale, which is far below
//! the Q15 resolution the tables are quantised to.

/// Pi in Q32 radians.
pub(crate) const PI_Q32: i64 = 13_493_037_705;
/// Two pi in Q32 radians.
pub(crate) const TAU_Q32: i64 = 26_986_075_409;
/// One in Q30.
pub(crate) const Q30_ONE: i64 = 1 << 30;

/// Sine of `radians` (Q32 radians) in Q32.
///
/// The argument is first reduced to `-pi..=pi` with an exact integer modulus, then evaluated as
/// `x - x^3/3! + x^5/5! - ... - x^15/15!`. Terms below the Q32 resolution are dropped.
pub(crate) const fn sin_q32(radians: i64) -> i64 {
    let mut x = radians % TAU_Q32;
    if x > PI_Q32 {
        x -= TAU_Q32;
    } else if x < -PI_Q32 {
        x += TAU_Q32;
    }
    let value = x as i128;
    let square = (value * value) >> 32;
    let mut sum = value;
    let mut term = value;
    let mut order = 1i128;
    while order <= 7 {
        term = -((term * square) >> 32) / (order * 2 * (order * 2 + 1));
        sum += term;
        order += 1;
    }
    sum as i64
}

/// Cosine of `radians` (Q32 radians) in Q32.
pub(crate) const fn cos_q32(radians: i64) -> i64 {
    sin_q32(radians + PI_Q32 / 2)
}

/// Clamp an arbitrary integer to the signed 16-bit sample range.
pub(crate) const fn clamp_i16(value: i64) -> i16 {
    if value > i16::MAX as i64 {
        i16::MAX
    } else if value < i16::MIN as i64 {
        i16::MIN
    } else {
        value as i16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One full turn expressed as Q32 radians.
    const QUARTER: i64 = PI_Q32 / 2;

    #[test]
    fn sine_hits_the_exact_cardinal_points() {
        let zero = sin_q32(0);
        let quarter = sin_q32(QUARTER);
        let half = sin_q32(PI_Q32);
        let three_quarters = sin_q32(PI_Q32 + QUARTER);
        // The Taylor series is truncated after x^15, which leaves a few thousand Q32 units
        // (about 1e-6 of full scale) at the zeros.
        assert!(zero.abs() < 1 << 13, "sin(0) = {zero}");
        assert!(
            (quarter - (1i64 << 32)).abs() < 1 << 13,
            "sin(pi/2) = {quarter}"
        );
        assert!(half.abs() < 1 << 13, "sin(pi) = {half}");
        assert!(
            (three_quarters + (1i64 << 32)).abs() < 1 << 13,
            "sin(3pi/2) = {three_quarters}"
        );
    }

    #[test]
    fn sine_is_periodic_and_odd() {
        for step in 0..64 {
            let angle = step * TAU_Q32 / 64;
            let value = sin_q32(angle);
            let shifted = sin_q32(angle + TAU_Q32);
            let mirrored = sin_q32(-angle);
            assert!((value - shifted).abs() <= 4, "period at {step}");
            assert!((value + mirrored).abs() <= 4, "odd symmetry at {step}");
        }
    }

    #[test]
    fn cosine_matches_the_shifted_sine() {
        for step in 0..32 {
            let angle = step * TAU_Q32 / 32;
            assert!((cos_q32(angle) - sin_q32(angle + QUARTER)).abs() <= 4);
        }
        assert!((cos_q32(0) - (1i64 << 32)).abs() < 1 << 10);
    }

    #[test]
    fn clamp_covers_the_sample_range() {
        assert_eq!(clamp_i16(i64::MIN), i16::MIN);
        assert_eq!(clamp_i16(-32_769), i16::MIN);
        assert_eq!(clamp_i16(-12), -12);
        assert_eq!(clamp_i16(0), 0);
        assert_eq!(clamp_i16(32_768), i16::MAX);
        assert_eq!(clamp_i16(i64::MAX), i16::MAX);
    }
}
