// SPDX-License-Identifier: MIT
// Copyright (c) 2018 Jorge Aparicio
// RavenBSD: transpiled from rust:library/compiler-builtins/libm/src/math/sincosf.rs @ 8bab26f4f68e0e26f0bb7960be334d5b520ea452
// (donor unit tests moved out of sincosf.rs per handbook rule 9)

use super::sincosf;

#[test]
fn rotational_symmetry() {
    use core::f32::consts::PI;
    const N: usize = 24;
    for n in 0..N {
        let theta = 2. * PI * (n as f32) / (N as f32);
        let (s, c) = sincosf(theta);
        let (s_plus, c_plus) = sincosf(theta + 2. * PI);
        let (s_minus, c_minus) = sincosf(theta - 2. * PI);

        const TOLERANCE: f32 = 1e-6;
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
