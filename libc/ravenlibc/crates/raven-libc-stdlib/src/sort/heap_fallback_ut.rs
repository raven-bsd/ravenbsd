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

//! The CLEANROOM in-place heap fallback: sortedness, permutation and zero allocations.

use super::heap_fallback;

fn value(elem: &[u8]) -> u64 {
    let mut value = 0u64;
    for (index, byte) in elem.iter().take(8).enumerate() {
        value |= u64::from(*byte) << (index * 8);
    }
    value
}

fn cmp(a: &[u8], b: &[u8]) -> i32 {
    match value(a).cmp(&value(b)) {
        core::cmp::Ordering::Less => -1,
        core::cmp::Ordering::Equal => 0,
        core::cmp::Ordering::Greater => 1,
    }
}

fn encode(values: &[u64], es: usize) -> Vec<u8> {
    let mut buffer = vec![0u8; values.len() * es];
    for (index, value) in values.iter().enumerate() {
        buffer[index * es..index * es + 8].copy_from_slice(&value.to_le_bytes());
    }
    buffer
}

fn decode(buffer: &[u8], es: usize) -> Vec<u64> {
    (0..buffer.len() / es)
        .map(|index| value(&buffer[index * es..index * es + 8]))
        .collect()
}

#[test]
fn sorts_every_distribution() {
    for n in [0usize, 1, 2, 3, 7, 8, 40, 41, 1000, 100_000] {
        for kind in 0..4u64 {
            let values: Vec<u64> = (0..n as u64)
                .map(|index| match kind {
                    0 => index,
                    1 => n as u64 - index,
                    2 => 7,
                    _ => index.wrapping_mul(2_654_435_761) % 997,
                })
                .collect();
            let mut buffer = encode(&values, 8);
            heap_fallback(&mut buffer, 8, 0, n, &mut cmp);
            let mut expected = values.clone();
            expected.sort_unstable();
            assert_eq!(decode(&buffer, 8), expected, "n={n} kind={kind}");
        }
    }
}

#[test]
fn subrange_only() {
    let values = vec![9u64, 5, 3, 8, 1, 2, 0, 7, 6, 4];
    let mut buffer = encode(&values, 8);
    heap_fallback(&mut buffer, 8, 2, 6, &mut cmp);
    let sorted = decode(&buffer, 8);
    assert_eq!(&sorted[..2], &[9, 5]);
    assert_eq!(&sorted[2..8], &[0, 1, 2, 3, 7, 8]);
    assert_eq!(&sorted[8..], &[6, 4]);
}

#[test]
fn es_zero_and_one_element_are_noops() {
    let mut empty: [u8; 0] = [];
    heap_fallback(&mut empty, 0, 0, 0, &mut cmp);
    let mut one = encode(&[1], 8);
    heap_fallback(&mut one, 8, 0, 1, &mut cmp);
    assert_eq!(decode(&one, 8), vec![1]);
}

#[test]
fn malformed_length_is_survivable() {
    let mut buffer = encode(&[3, 1, 2], 8);
    // The caller promised three elements; claim ten: the engine must stay in bounds.
    heap_fallback(&mut buffer, 8, 0, 10, &mut cmp);
    assert_eq!(buffer.len(), 24);
}
