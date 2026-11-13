// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::assertions_on_constants
)]

//! `rand`/`srand`: determinism, the default state and the glibc-compatible seed.

use super::*;

#[test]
fn rand_max_is_31_bits() {
    assert_eq!(RAND_MAX, 0x7fff_ffff);
}

#[test]
fn first_values_after_srand_one() {
    // Measured from the NetBSD random.c donor with USE_BETTER_RANDOM (see the C41 report and
    // the stdlib-netbsd differential); TYPE_3, seed 1.
    let expected: [u32; 10] = [
        1_804_289_383,
        846_930_886,
        1_681_692_777,
        1_714_636_915,
        1_957_747_793,
        424_238_335,
        719_885_386,
        1_649_760_492,
        596_516_649,
        1_189_641_421,
    ];
    let mut generator = DefaultRandom::new();
    generator.srand(1);
    for value in expected {
        assert_eq!(generator.rand(), value);
    }
}

#[test]
fn default_state_matches_srand_one() {
    let mut fresh = DefaultRandom::new();
    let mut seeded = DefaultRandom::new();
    seeded.srand(1);
    for _ in 0..100 {
        assert_eq!(fresh.rand(), seeded.rand());
    }
}

#[test]
fn seeded_sequences_are_reproducible() {
    for seed in [0u32, 1, 2, 42, u32::MAX] {
        let mut a = DefaultRandom::new();
        let mut b = DefaultRandom::new();
        a.srand(seed);
        b.srand(seed);
        for _ in 0..1000 {
            assert_eq!(a.rand(), b.rand());
        }
    }
}

#[test]
fn srand_zero_is_srand_one() {
    let mut zero = DefaultRandom::new();
    let mut one = DefaultRandom::new();
    zero.srand(0);
    one.srand(1);
    for _ in 0..100 {
        assert_eq!(zero.rand(), one.rand());
    }
}

#[test]
fn different_seeds_differ() {
    let mut a = DefaultRandom::new();
    let mut b = DefaultRandom::new();
    a.srand(2);
    b.srand(3);
    let mut differs = false;
    for _ in 0..10 {
        if a.rand() != b.rand() {
            differs = true;
            break;
        }
    }
    assert!(differs);
}

#[test]
fn free_functions_use_the_same_state() {
    let mut generator = DefaultRandom::new();
    srand(&mut generator, 5);
    let mut other = DefaultRandom::new();
    other.srand(5);
    assert_eq!(rand(&mut generator), other.rand());
}
