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

//! `heapsort` vectors from NetBSD `h_sort.c` and the error paths.

use super::heapsort;

fn cmp(a: &[u8], b: &[u8]) -> i32 {
    let mut left = 0u64;
    let mut right = 0u64;
    for (index, byte) in a.iter().take(8).enumerate() {
        left |= u64::from(*byte) << (index * 8);
    }
    for (index, byte) in b.iter().take(8).enumerate() {
        right |= u64::from(*byte) << (index * 8);
    }
    match left.cmp(&right) {
        core::cmp::Ordering::Less => -1,
        core::cmp::Ordering::Equal => 0,
        core::cmp::Ordering::Greater => 1,
    }
}

fn encode(values: &[u64], es: usize) -> Vec<u8> {
    let mut buffer = vec![0u8; values.len() * es];
    for (index, value) in values.iter().enumerate() {
        let take = core::cmp::min(es, 8);
        buffer[index * es..index * es + take].copy_from_slice(&value.to_le_bytes()[..take]);
    }
    buffer
}

fn decode(buffer: &[u8], es: usize) -> Vec<u64> {
    (0..buffer.len() / es)
        .map(|index| {
            let mut value = 0u64;
            let take = core::cmp::min(es, 8);
            for (offset, byte) in buffer[index * es..index * es + take].iter().enumerate() {
                value |= u64::from(*byte) << (offset * 8);
            }
            value
        })
        .collect()
}

#[test]
fn sorts_vectors() {
    for n in [0usize, 1, 2, 3, 7, 8, 15, 100, 1000] {
        let values: Vec<u64> = (0..n as u64).map(|index| (index * 37) % 101).collect();
        let mut buffer = encode(&values, 8);
        heapsort(&mut buffer, 8, cmp).unwrap();
        let mut expected = values.clone();
        expected.sort_unstable();
        assert_eq!(decode(&buffer, 8), expected, "n={n}");
    }
}

#[test]
fn every_element_size() {
    for es in 4..=33usize {
        let values: Vec<u64> = vec![9, 1, 8, 2, 7, 3, 6, 4, 5, 0];
        let mut buffer = encode(&values, es);
        heapsort(&mut buffer, es, cmp).unwrap();
        let sorted = decode(&buffer, es);
        assert!(sorted.windows(2).all(|pair| pair[0] <= pair[1]), "es={es}");
    }
}

#[test]
fn zero_size_is_invalid_only_for_many_elements() {
    let mut buffer = vec![0u8; 16];
    assert_eq!(heapsort(&mut buffer, 0, cmp), Err(super::SortError::Inval));
    let mut empty: [u8; 0] = [];
    assert_eq!(heapsort(&mut empty, 0, cmp), Ok(()));
}

#[test]
fn reversed_input() {
    let values: Vec<u64> = (0..128u64).rev().collect();
    let mut buffer = encode(&values, 16);
    heapsort(&mut buffer, 16, cmp).unwrap();
    assert_eq!(decode(&buffer, 16), (0..128).collect::<Vec<u64>>());
}
