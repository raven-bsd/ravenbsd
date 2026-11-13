// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! C23 bit-level tests: total ordering, payloads, canonicalisation and neighbours.

use super::*;

#[test]
fn the_total_order_places_nan_and_zero() {
    // -NaN (-inf side) < -inf < -1 < -0 < +0 < 1 < +inf < +NaN
    let series = [
        f64::from_bits(0xfff8_0000_0000_0000),
        f64::NEG_INFINITY,
        -1.0,
        -0.0,
        0.0,
        1.0,
        f64::INFINITY,
        f64::from_bits(0x7ff8_0000_0000_0000),
    ];
    for window in series.windows(2) {
        assert!(
            totalorder(window[0], window[1]),
            "{:?} <= {:?}",
            window[0],
            window[1]
        );
        assert!(!totalorder(window[1], window[0]));
    }
    assert!(totalorder(1.0, 1.0));
    assert!(!totalorder(f64::NAN, f64::INFINITY));
}

#[test]
fn nan_payloads_order_inside_the_nan_range() {
    let small = f64::from_bits(0x7ff8_0000_0000_0001);
    let large = f64::from_bits(0x7ff8_0000_0000_0002);
    assert!(totalorder(small, large));
    assert!(!totalorder(large, small));
    assert!(totalorderf(
        f32::from_bits(0x7fc0_0001),
        f32::from_bits(0x7fc0_0002)
    ));
}

#[test]
fn magnitude_order_ignores_the_sign_until_a_tie() {
    assert!(!totalordermag(-3.0, 2.0));
    assert!(!totalordermag(3.0, -2.0));
    assert!(totalordermag(-0.0, 0.0));
    assert!(!totalordermag(0.0, -0.0));
    assert!(!totalordermag(-1.0, -1.0));
    assert!(totalordermagf(-2.0, 3.0));
}

#[test]
fn canonicalize_returns_the_quiet_nan() {
    let signalling = f64::from_bits(0x7ff0_0000_0000_0001);
    assert!(issignaling(signalling));
    let canonical = canonicalize(signalling);
    assert!(canonical.is_nan());
    assert!(!issignaling(canonical));
    assert_eq!(canonicalize(2.5), 2.5);
    assert!(canonicalizef(f32::from_bits(0x7f80_0001)).is_nan());
}

#[test]
fn payloads_round_trip_through_the_nan_bits() {
    assert_eq!(getpayload(f64::NAN), F64_MANTISSA & f64::NAN.to_bits());
    let value = setpayload(0.0, 0x1234);
    assert!(value.is_nan());
    assert_eq!(getpayload(value) & !F64_QUIET, 0x1234);
    assert_eq!(setpayload(0.0, 0), 0.0);
    assert!(setpayload(0.0, F64_PAYLOAD_MAX).is_nan());
    let signalling = setpayloadsig(0.0, 7);
    assert!(issignaling(signalling));
    assert_eq!(getpayload(signalling), 7);
    assert_eq!(setpayloadsigf(0.0, 0), 0.0);
    let small = setpayloadf(0.0, 5);
    assert!(small.is_nan());
    assert_eq!(getpayloadf(small), F32_QUIET | 5);
}

#[test]
fn neighbours_step_one_bit() {
    assert_eq!(nextup(0.0), f64::from_bits(1));
    assert_eq!(nextdown(0.0), -f64::from_bits(1));
    assert_eq!(nextup(-0.0), f64::from_bits(1));
    assert_eq!(nextup(1.0).to_bits(), 1.0f64.to_bits() + 1);
    assert_eq!(nextdown(1.0).to_bits(), 1.0f64.to_bits() - 1);
    assert_eq!(nextup(f64::INFINITY), f64::INFINITY);
    assert_eq!(nextdown(f64::NEG_INFINITY), f64::NEG_INFINITY);
    assert!(nextup(f64::NAN).is_nan());
    assert_eq!(nextup(1.0), 1.0 + f64::EPSILON);
}

#[test]
fn roundeven_breaks_ties_to_even() {
    assert_eq!(roundeven(0.5), 0.0);
    assert_eq!(roundeven(1.5), 2.0);
    assert_eq!(roundeven(2.5), 2.0);
    assert_eq!(roundeven(-0.5), -0.0);
    assert_eq!(roundeven(-1.5), -2.0);
    assert_eq!(roundeven(-2.5), -2.0);
    assert_eq!(roundeven(2.4), 2.0);
    assert_eq!(roundeven(2.6), 3.0);
    assert_eq!(roundeven(f64::INFINITY), f64::INFINITY);
}
