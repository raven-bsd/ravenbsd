// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `sinpi` exact values, symmetry and special cases.

use super::{sinpi, sinpif};

#[test]
fn integers_give_signed_zero() {
    assert_eq!(sinpi(0.0), 0.0);
    assert_eq!(sinpi(-0.0), -0.0);
    assert_eq!(sinpi(1.0), 0.0);
    assert_eq!(sinpi(-1.0), -0.0);
    assert_eq!(sinpi(2.0), 0.0);
    assert_eq!(sinpi(-2.0), -0.0);
    assert_eq!(sinpi(1e18), 0.0);
    assert_eq!(sinpif(-3.0), -0.0);
}

#[test]
fn halves_are_exact_and_alternate() {
    assert_eq!(sinpi(0.5), 1.0);
    assert_eq!(sinpi(-0.5), -1.0);
    assert_eq!(sinpi(1.5), -1.0);
    assert_eq!(sinpi(2.5), 1.0);
    assert_eq!(sinpi(-1.5), 1.0);
    assert_eq!(sinpif(0.5), 1.0);
    assert_eq!(sinpif(1.5), -1.0);
}

#[test]
fn special_values() {
    assert!(sinpi(f64::NAN).is_nan());
    assert!(sinpi(f64::INFINITY).is_nan());
    assert!(sinpi(f64::NEG_INFINITY).is_nan());
    assert_eq!(
        sinpi(f64::from_bits(1)),
        super::sinpi(0.0) + f64::from_bits(1) * core::f64::consts::PI
    );
}

#[test]
fn symmetry_and_small_values() {
    for index in -999..=999 {
        let x = f64::from(index) / 512.0;
        assert_eq!(sinpi(-x), -sinpi(x), "sinpi({x})");
        let reference = (core::f64::consts::PI * x).sin();
        assert!(
            (sinpi(x) - reference).abs() <= 4.0 * f64::EPSILON,
            "sinpi({x}) = {} vs {reference}",
            sinpi(x)
        );
    }
    for index in -999..=999 {
        let x = f32::from(index as i16) / 512.0;
        let reference = (core::f32::consts::PI * x).sin();
        assert!(
            (sinpif(x) - reference).abs() <= 4.0 * f32::EPSILON,
            "sinpif({x})"
        );
    }
}
