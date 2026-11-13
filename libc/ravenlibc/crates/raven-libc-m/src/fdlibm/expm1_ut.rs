// SPDX-License-Identifier: MIT
// Copyright (c) 2018 Jorge Aparicio
// RavenBSD: transpiled from rust:library/compiler-builtins/libm/src/math/expm1.rs @ 8bab26f4f68e0e26f0bb7960be334d5b520ea452
// (donor sanity test kept; extra special values added here)

use super::expm1;

#[test]
fn sanity_check() {
    assert_eq!(expm1(1.1), 2.0041660239464334);
}

#[test]
fn special_values() {
    assert_eq!(expm1(0.0), 0.0);
    assert_eq!(expm1(-0.0), -0.0);
    assert_eq!(expm1(f64::NEG_INFINITY), -1.0);
    assert_eq!(expm1(f64::INFINITY), f64::INFINITY);
    assert!(expm1(f64::NAN).is_nan());
    // overflow
    assert_eq!(expm1(710.0), f64::INFINITY);
    // tiny arguments return the argument (within an ULP)
    let tiny = 1e-300;
    assert_eq!(expm1(tiny), tiny);
}
