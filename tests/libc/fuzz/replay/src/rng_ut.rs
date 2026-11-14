// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn seed_zero_vector_matches_c35() {
    let mut rng = Rng::seed_from_u64(0);
    assert_eq!(rng.next_u64(), 0x99ec_5f36_cb75_f2b4);
    assert_eq!(rng.next_u64(), 0xbf6e_1f78_4956_452a);
    assert_eq!(rng.next_u64(), 0x1a5f_849d_4933_e6e0);
    assert_eq!(rng.next_u64(), 0x6aa5_94f1_262d_2d2c);
}

#[test]
fn seed_one_vector_matches_c35() {
    let mut rng = Rng::seed_from_u64(1);
    assert_eq!(rng.next_u64(), 0xb3f2_af6d_0fc7_10c5);
    assert_eq!(rng.next_u64(), 0x853b_5596_4736_4cea);
}

#[test]
fn seed_forty_two_vector_matches_c35() {
    let mut rng = Rng::seed_from_u64(42);
    assert_eq!(rng.next_u64(), 0x1578_0b2e_0c2e_c716);
    assert_eq!(rng.next_u64(), 0x6104_d986_6d11_3a7e);
}

#[test]
fn equal_seeds_agree() {
    let mut left = Rng::seed_from_u64(7);
    let mut right = Rng::seed_from_u64(7);
    assert_eq!(left.next_u64(), right.next_u64());
    assert_eq!(left.bytes(13), right.bytes(13));
}

#[test]
fn different_seeds_differ() {
    let mut left = Rng::seed_from_u64(7);
    let mut right = Rng::seed_from_u64(8);
    assert_ne!(left.next_u64(), right.next_u64());
}

#[test]
fn below_stays_in_range() {
    let mut rng = Rng::seed_from_u64(3);
    for n in [1u64, 2, 3, 7, 255, 256, 1000] {
        for _ in 0..32 {
            assert!(rng.below(n) < n);
        }
    }
    assert_eq!(rng.below(0), 0);
}

#[test]
fn bytes_are_reproducible() {
    let mut first = Rng::seed_from_u64(123);
    let mut second = Rng::seed_from_u64(123);
    assert_eq!(first.bytes(97), second.bytes(97));
}

#[test]
fn bytes_have_the_requested_length() {
    let mut rng = Rng::seed_from_u64(9);
    for length in [0usize, 1, 7, 8, 9, 64, 4096] {
        assert_eq!(rng.bytes(length).len(), length);
    }
}
