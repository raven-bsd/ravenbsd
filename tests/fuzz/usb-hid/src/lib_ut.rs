// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the deterministic generator.

use super::*;

#[test]
fn identical_seeds_produce_identical_sequences() {
    let mut first = Lcg::new(7);
    let mut second = Lcg::new(7);
    for _ in 0..64 {
        assert_eq!(first.next_u32(), second.next_u32());
    }
}

#[test]
fn below_stays_in_range() {
    let mut generator = Lcg::new(1);
    for bound in [0usize, 1, 2, 7, 256] {
        for _ in 0..100 {
            assert!(generator.below(bound) < bound.max(1));
        }
    }
}

#[test]
fn fill_covers_the_slice() {
    let mut generator = Lcg::new(1);
    let mut buffer = [0u8; 16];
    generator.fill(&mut buffer);
    assert!(buffer.iter().any(|byte| *byte != 0));
}
