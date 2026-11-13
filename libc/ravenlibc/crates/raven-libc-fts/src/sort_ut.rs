// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for the tolerant merge sort.
// Test-only code is exempt from the production lints (17-crate-layout 9.3).
#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use super::*;

#[test]
fn sorts_ascending_and_descending() {
    let mut values = alloc::vec![3, 1, 2, 5, 4];
    merge_sort(&mut values, &mut |a, b| *a - *b);
    assert_eq!(values, alloc::vec![1, 2, 3, 4, 5]);
    merge_sort(&mut values, &mut |a, b| *b - *a);
    assert_eq!(values, alloc::vec![5, 4, 3, 2, 1]);
}

#[test]
fn stable_for_equal_keys() {
    let mut values = alloc::vec![(1, 'a'), (1, 'b'), (1, 'c')];
    merge_sort(&mut values, &mut |a, b| a.0 - b.0);
    assert_eq!(values, alloc::vec![(1, 'a'), (1, 'b'), (1, 'c')]);
}

#[test]
fn an_inconsistent_comparator_does_not_panic_or_loop() {
    let mut values: alloc::vec::Vec<u32> = (0..257).collect();
    let mut counter = 0u32;
    let mut weird = |_a: &u32, _b: &u32| {
        counter = counter.wrapping_mul(1103515245).wrapping_add(12345);
        match counter % 3 {
            0 => -1,
            1 => 0,
            _ => 1,
        }
    };
    merge_sort(&mut values, &mut weird);
    assert_eq!(values.len(), 257);
}

#[test]
fn an_always_equal_comparator_keeps_everything() {
    let mut values: alloc::vec::Vec<u32> = (0..100).collect();
    merge_sort(&mut values, &mut |_a, _b| 0);
    assert_eq!(values, (0..100).collect::<alloc::vec::Vec<u32>>());
}

#[test]
fn short_and_empty_inputs() {
    let mut empty: alloc::vec::Vec<u32> = alloc::vec![];
    merge_sort(&mut empty, &mut |_a, _b| 0);
    assert!(empty.is_empty());
    let mut one = alloc::vec![7u32];
    merge_sort(&mut one, &mut |_a, _b| 0);
    assert_eq!(one, alloc::vec![7]);
}
