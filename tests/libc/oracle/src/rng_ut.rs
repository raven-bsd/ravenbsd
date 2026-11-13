// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

fn first_four(seed: u64) -> [u64; 4] {
    let mut rng = Rng::seed_from_u64(seed);
    [
        rng.next_u64(),
        rng.next_u64(),
        rng.next_u64(),
        rng.next_u64(),
    ]
}

#[test]
fn the_three_reference_vectors_match() {
    assert_eq!(
        first_four(0),
        [
            0x99ec_5f36_cb75_f2b4,
            0xbf6e_1f78_4956_452a,
            0x1a5f_849d_4933_e6e0,
            0x6aa5_94f1_262d_2d2c,
        ]
    );
    assert_eq!(
        first_four(1),
        [
            0xb3f2_af6d_0fc7_10c5,
            0x853b_5596_4736_4cea,
            0x92f8_9756_082a_4514,
            0x642e_1c7b_c266_a3a7,
        ]
    );
    assert_eq!(
        first_four(42),
        [
            0x1578_0b2e_0c2e_c716,
            0x6104_d986_6d11_3a7e,
            0xae17_5332_39e4_99a1,
            0xecb8_ad47_03b3_60a1,
        ]
    );
}

#[test]
fn equal_seeds_agree_and_different_seeds_differ() {
    let mut left = Rng::seed_from_u64(7);
    let mut right = Rng::seed_from_u64(7);
    assert_eq!(left.next_u64(), right.next_u64());
    assert_eq!(left.bytes(13), right.bytes(13));
    let mut other = Rng::seed_from_u64(8);
    assert_ne!(left.next_u64(), other.next_u64());
}

#[test]
fn below_stays_in_range() {
    let mut rng = Rng::seed_from_u64(3);
    for n in [1u64, 2, 3, 7, 255, 256, 1000] {
        for _ in 0..100 {
            assert!(rng.below(n) < n);
        }
    }
    assert_eq!(rng.below(0), 0);
}

#[test]
fn bytes_has_the_requested_length() {
    let mut rng = Rng::seed_from_u64(9);
    for n in [0usize, 1, 7, 8, 9, 100] {
        assert_eq!(rng.bytes(n).len(), n);
    }
}

#[test]
fn pick_returns_an_element_of_the_slice() {
    let mut rng = Rng::seed_from_u64(11);
    let items = [10, 20, 30];
    for _ in 0..100 {
        assert!(items.contains(&rng.pick(&items)));
    }
}
