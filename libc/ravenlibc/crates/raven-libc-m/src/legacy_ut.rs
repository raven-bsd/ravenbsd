// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Legacy alias tests: each alias must agree with its C99 entry point.

use super::*;

#[test]
fn sincos_returns_both_values() {
    for x in [-2.0, -0.5, 0.0, 0.5, 2.0] {
        let (s, c) = sincos(x);
        assert_eq!(s, crate::sin(x));
        assert_eq!(c, crate::cos(x));
    }
    let (s, c) = sincosf(0.5);
    assert_eq!(s, crate::sinf(0.5));
    assert_eq!(c, crate::cosf(0.5));
}

#[test]
fn pow10_is_exp10() {
    assert_eq!(pow10(2.0), 100.0);
    assert_eq!(pow10(-1.0), 0.1);
    assert_eq!(pow10f(2.0), 100.0);
    assert_eq!(pow10f(0.5), crate::exp10f(0.5));
    assert_eq!(pow10(0.0), 1.0);
}

#[test]
fn finite_is_isfinite() {
    assert!(finite(0.0));
    assert!(finite(-1e300));
    assert!(!finite(f64::INFINITY));
    assert!(!finite(f64::NAN));
    assert!(finitef(1.0));
    assert!(!finitef(f32::NEG_INFINITY));
}

#[test]
fn gamma_is_lgamma() {
    for x in [0.5, 1.0, 2.5, 10.0] {
        assert_eq!(gamma(x), crate::lgamma(x));
        assert_eq!(gammaf(x as f32), crate::lgammaf(x as f32));
    }
}

#[test]
fn drem_is_the_ieee_remainder() {
    assert_eq!(drem(5.0, 2.0), 1.0);
    assert_eq!(drem(6.0, 2.0), 0.0);
    assert_eq!(drem(5.5, 2.0), -0.5);
    assert_eq!(drem(-5.5, 2.0), 0.5);
    assert_eq!(dremf(5.5, 2.0), -0.5);
    assert!(drem(1.0, 0.0).is_nan());
}

#[test]
fn scalb_scales_by_a_power_of_two() {
    assert_eq!(scalb(1.5, 3.0), 12.0);
    assert_eq!(scalb(1.0, -2.0), 0.25);
    assert_eq!(scalbf(1.5, 3.0), 12.0);
    assert_eq!(scalb(f64::INFINITY, 4.0), f64::INFINITY);
}

#[test]
fn significand_reduces_to_the_unit_interval() {
    assert_eq!(significand(1.0), 1.0);
    assert_eq!(significand(2.0), 1.0);
    assert_eq!(significand(7.0), 1.75);
    assert_eq!(significand(-7.0), -1.75);
    assert_eq!(significand(0.0), 0.0);
    assert_eq!(significand(f64::INFINITY), f64::INFINITY);
    assert_eq!(significandf(7.0), 1.75);
}
