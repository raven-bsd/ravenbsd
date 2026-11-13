// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! C23 `fmaximum_mag`/`fminimum_mag` cases.

use super::{fmaximum_mag, fminimum_mag};

#[test]
fn the_greater_magnitude_wins() {
    assert_eq!(fmaximum_mag(-3.0f64, 2.0f64), -3.0);
    assert_eq!(fmaximum_mag(2.0f64, -3.0f64), -3.0);
    assert_eq!(fminimum_mag(-3.0f64, 2.0f64), 2.0);
    assert_eq!(fminimum_mag(2.0f64, -3.0f64), 2.0);
}

#[test]
fn equal_magnitudes_fall_back_to_maximum_and_minimum() {
    assert_eq!(fmaximum_mag(0.0f64, -0.0f64), 0.0);
    assert_eq!(fmaximum_mag(-0.0f64, 0.0f64), 0.0);
    assert_eq!(fminimum_mag(0.0f64, -0.0f64), -0.0);
    assert_eq!(fminimum_mag(-2.0f64, 2.0f64), -2.0);
    assert_eq!(fmaximum_mag(-2.0f64, 2.0f64), 2.0);
}

#[test]
fn a_nan_argument_returns_the_other_one() {
    assert_eq!(fmaximum_mag(f64::NAN, 1.5f64), 1.5);
    assert_eq!(fmaximum_mag(1.5f64, f64::NAN), 1.5);
    assert_eq!(fminimum_mag(f64::NAN, -1.5f64), -1.5);
    assert_eq!(fminimum_mag(-1.5f64, f64::NAN), -1.5);
    assert!(fmaximum_mag(f64::NAN, f64::NAN).is_nan());
}

#[test]
fn the_f32_forms_match() {
    assert_eq!(fmaximum_mag(-3.0f32, 2.0f32), -3.0);
    assert_eq!(fminimum_mag(-3.0f32, 2.0f32), 2.0);
    assert_eq!(fmaximum_mag(f32::NAN, 1.5f32), 1.5);
}
