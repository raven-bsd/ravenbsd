// SPDX-License-Identifier: MIT
// Copyright (c) 2018 Jorge Aparicio
// RavenBSD: transpiled from rust:library/compiler-builtins/libm/src/math/sincos.rs @ 8bab26f4f68e0e26f0bb7960be334d5b520ea452
// (donor unit tests moved out of sincos.rs per handbook rule 9)

use super::sincos;

const TOLERANCE: f64 = 1e-6;

#[test]
fn with_pi() {
    let (s, c) = sincos(core::f64::consts::PI);
    assert!(
        (s - 0.0).abs() < TOLERANCE,
        "|{} - {}| = {} >= {}",
        s,
        0.0,
        (s - 0.0).abs(),
        TOLERANCE
    );
    assert!(
        (c + 1.0).abs() < TOLERANCE,
        "|{} + {}| = {} >= {}",
        c,
        1.0,
        (s + 1.0).abs(),
        TOLERANCE
    );
}

#[test]
fn rotational_symmetry() {
    use core::f64::consts::PI;
    const N: usize = 24;
    for n in 0..N {
        let theta = 2. * PI * (n as f64) / (N as f64);
        let (s, c) = sincos(theta);
        let (s_plus, c_plus) = sincos(theta + 2. * PI);
        let (s_minus, c_minus) = sincos(theta - 2. * PI);

        assert!(
            (s - s_plus).abs() < TOLERANCE,
            "|{} - {}| = {} >= {}",
            s,
            s_plus,
            (s - s_plus).abs(),
            TOLERANCE
        );
        assert!(
            (s - s_minus).abs() < TOLERANCE,
            "|{} - {}| = {} >= {}",
            s,
            s_minus,
            (s - s_minus).abs(),
            TOLERANCE
        );
        assert!(
            (c - c_plus).abs() < TOLERANCE,
            "|{} - {}| = {} >= {}",
            c,
            c_plus,
            (c - c_plus).abs(),
            TOLERANCE
        );
        assert!(
            (c - c_minus).abs() < TOLERANCE,
            "|{} - {}| = {} >= {}",
            c,
            c_minus,
            (c - c_minus).abs(),
            TOLERANCE
        );
    }
}
