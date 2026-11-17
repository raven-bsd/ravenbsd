// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Cases for the C23/C2x additions in [`super`].

use super::*;

fn close(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance
}

#[test]
fn nextafter_steps_one_ulp_and_preserves_the_zero_sign() {
    assert_eq!(nextafter(1.0, 2.0), 1.0 + f64::EPSILON);
    assert_eq!(nextafter(1.0, 0.0), 1.0 - f64::EPSILON / 2.0);
    assert_eq!(nextafter(1.0, 1.0), 1.0);
    assert_eq!(
        nextafter(0.0, -1.0).to_bits(),
        (-f64::from_bits(1)).to_bits()
    );
    assert!(nextafter(f64::NAN, 1.0).is_nan());
    assert_eq!(nextafterf(1.0, 2.0), 1.0 + f32::EPSILON);
}

#[test]
fn nexttoward_uses_long_double_direction() {
    let two = LongDouble::from_f64(2.0);
    assert_eq!(nexttoward(1.0, two), 1.0 + f64::EPSILON);
    assert_eq!(nexttoward(1.0, LongDouble::from_f64(1.0)), 1.0);
    assert_eq!(nexttowardf(1.0, two), 1.0 + f32::EPSILON);
}

#[test]
fn logb_and_llogb_report_the_exponent() {
    assert_eq!(logb(8.0), 3.0);
    assert_eq!(logb(0.5), -1.0);
    assert_eq!(logb(0.0), f64::NEG_INFINITY);
    assert_eq!(logb(f64::INFINITY), f64::INFINITY);
    assert_eq!(llogb(8.0), 3);
    assert_eq!(llogb(0.0), i64::MIN);
    assert_eq!(llogb(f64::INFINITY), i64::MAX);
    assert_eq!(logbf(8.0), 3.0);
}

#[test]
fn scalbln_scales_by_powers_of_two() {
    assert_eq!(scalbln(1.5, 3), 12.0);
    assert_eq!(scalbln(1.5, -2), 0.375);
    assert_eq!(scalblnf(1.5, 3), 12.0);
}

#[test]
fn integer_rounding_families_follow_their_modes() {
    assert_eq!(lrint(2.5), 2);
    assert_eq!(llrint(3.5), 4);
    assert_eq!(lround(2.5), 3);
    assert_eq!(llround(-2.5), -3);
    assert_eq!(lrintf(-2.5), -2);
    assert_eq!(lroundf(-2.5), -3);
    assert_eq!(nearbyint(2.5), 2.0);
    assert_eq!(nearbyintf(-2.5), -2.0);
}

#[test]
fn remainder_and_remquo_choose_the_nearest_quotient() {
    assert_eq!(remainder(5.5, 2.0), -0.5);
    assert_eq!(remainder(-5.5, 2.0), 0.5);
    assert_eq!(remainder(4.0, 2.0), 0.0);
    assert_eq!(remainderf(5.5, 2.0), -0.5);
    let mut quotient = 0;
    assert_eq!(remquo(5.5, 2.0, &mut quotient), -0.5);
    assert_eq!(quotient, 3);
    let mut quotient = 0;
    assert_eq!(remquof(-5.5, 2.0, &mut quotient), 0.5);
    assert_eq!(quotient, -3);
}

#[test]
fn magnitude_aliases_and_the_number_forms() {
    assert_eq!(fmaxmag(-3.0, 2.0), -3.0);
    assert_eq!(fminmag(-3.0, 2.0), 2.0);
    assert_eq!(fmaxmagf(-3.0, 2.0), -3.0);
    assert_eq!(fminmagf(-3.0, 2.0), 2.0);
    assert_eq!(fmaximum_mag_num(f64::NAN, 1.5), 1.5);
    assert_eq!(fminimum_mag_num(1.5, f64::NAN), 1.5);
    assert_eq!(fmaximum_mag_numf(1.5, f32::NAN), 1.5);
    assert_eq!(fminimum_mag_numf(f32::NAN, 1.5), 1.5);
}

#[test]
fn integer_power_and_root_forms() {
    assert_eq!(pown(2.0, 10), 1024.0);
    assert_eq!(pown(2.0, -2), 0.25);
    assert_eq!(pown(f64::NAN, 0), 1.0);
    assert_eq!(pownf(3.0, 3), 27.0);
    assert_eq!(rootn(27.0, 3), 3.0);
    assert_eq!(rootn(-8.0, 3), -2.0);
    assert!(rootn(-4.0, 2).is_nan());
    assert_eq!(rootn(0.0, 5), 0.0);
    assert_eq!(rootnf(16.0, -2), 0.25);
}

#[test]
fn powr_and_compoundn_domains() {
    assert!(powr(-1.0, 0.5).is_nan());
    assert_eq!(powr(4.0, 0.5), 2.0);
    assert!(powrf(-1.0, 2.0).is_nan());
    assert!(close(compoundn(0.05, 2), 1.1025, 1.0e-12));
    assert!(close(compoundn(0.00001, 1000), 1.010_050_3, 1.0e-5));
    assert!(close(f64::from(compoundnf(0.05, 2)), 1.1025, 1.0e-6));
    assert_eq!(compoundn(-1.0, 2), 0.0);
    assert!(compoundn(-2.0, 2).is_nan());
}

#[test]
fn exp_and_log_shifted_forms_are_accurate_near_zero() {
    assert_eq!(exp10m1(0.0), 0.0);
    assert!(close(exp10m1(1.0e-9), 1.0e-9 * LN_10, 1.0e-15));
    assert_eq!(exp2m1(0.0), 0.0);
    assert!(close(exp2m1(1.0e-9), 1.0e-9 * LN_2, 1.0e-15));
    assert_eq!(log10p1(0.0), 0.0);
    assert!(close(log10p1(1.0e-9), 1.0e-9 * LOG10_E, 1.0e-15));
    assert_eq!(log2p1(0.0), 0.0);
    assert!(close(log2p1(1.0e-9), 1.0e-9 * LOG2_E, 1.0e-15));
    assert_eq!(exp10m1f(0.0), 0.0);
    assert_eq!(log10p1f(0.0), 0.0);
}

#[test]
fn pi_family_uses_exact_half_integers() {
    assert_eq!(cospi(0.0), 1.0);
    assert_eq!(cospi(0.5), 0.0);
    assert_eq!(cospi(1.0), -1.0);
    assert!(close(tanpi(0.25), 1.0, 1.0e-15));
    assert!(close(asinpi(1.0), 0.5, 1.0e-15));
    assert!(close(acospi(0.0), 0.5, 1.0e-15));
    assert!(close(atanpi(1.0), 0.25, 1.0e-15));
    assert!(close(atan2pi(1.0, 1.0), 0.25, 1.0e-15));
}

#[test]
fn fromfp_honours_mode_and_width() {
    assert_eq!(fromfp(2.5, FP_INT_TONEAREST, 8), 2);
    assert_eq!(fromfp(2.5, FP_INT_TONEARESTFROMZERO, 8), 3);
    assert_eq!(fromfp(-2.5, FP_INT_UPWARD, 8), -2);
    assert_eq!(fromfp(-2.5, FP_INT_DOWNWARD, 8), -3);
    assert_eq!(fromfp(2.5, FP_INT_TOWARDZERO, 8), 2);
    assert_eq!(fromfp(300.0, FP_INT_TONEAREST, 8), 0);
    assert_eq!(fromfpx(2.5, FP_INT_TONEAREST, 8), 2);
    assert_eq!(ufromfp(2.5, FP_INT_DOWNWARD, 8), 2);
    assert_eq!(ufromfpx(2.5, FP_INT_UPWARD, 8), 3);
    assert_eq!(ufromfp(-1.0, FP_INT_UPWARD, 8), 0);
}

#[test]
fn classification_function_symbols_match_the_macros() {
    assert_eq!(fpclassify(0.0), FP_ZERO);
    assert_eq!(
        fpclassify(4_503_599_627_370_496.0),
        crate::consts::FP_NORMAL
    );
    assert_eq!(isfinite(1.0), 1);
    assert_eq!(isnan(f64::NAN), 1);
    assert_eq!(isnormal(1.0), 1);
    assert_eq!(signbit(-0.0), 1);
    assert_eq!(isgreater(2.0, 1.0), 1);
    assert_eq!(isgreaterequal(2.0, 2.0), 1);
    assert_eq!(isless(1.0, 2.0), 1);
    assert_eq!(islessequal(2.0, 2.0), 1);
    assert_eq!(islessgreater(1.0, 2.0), 1);
    assert_eq!(isunordered(f64::NAN, 1.0), 1);
    assert_eq!(iseqsig(2.0, 2.0), 1);
    assert_eq!(iszero(-0.0), 1);
    assert_eq!(issubnormal(f64::from_bits(1)), 1);
    assert_eq!(iscanonical(1.0), 1);
}
