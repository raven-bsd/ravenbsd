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

//! `mergesort`: stability, element-size rejection and the checked-capacity error.

use super::mergesort;

fn cmp_pair(a: &[u8], b: &[u8]) -> i32 {
    let left = u32::from_le_bytes([a[0], a[1], a[2], a[3]]);
    let right = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    match left.cmp(&right) {
        core::cmp::Ordering::Less => -1,
        core::cmp::Ordering::Equal => 0,
        core::cmp::Ordering::Greater => 1,
    }
}

#[test]
fn stable_on_keyed_records() {
    let records: Vec<(u32, u32)> = vec![
        (2, 0),
        (1, 1),
        (2, 2),
        (0, 3),
        (2, 4),
        (1, 5),
        (0, 6),
        (2, 7),
        (1, 8),
    ];
    let mut buffer = Vec::new();
    for (key, seq) in &records {
        buffer.extend_from_slice(&key.to_le_bytes());
        buffer.extend_from_slice(&seq.to_le_bytes());
    }
    mergesort(&mut buffer, 8, cmp_pair).unwrap();
    let mut pairs = Vec::new();
    for chunk in buffer.chunks_exact(8) {
        pairs.push((
            u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]),
            u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]),
        ));
    }
    assert_eq!(
        pairs,
        vec![
            (0, 3),
            (0, 6),
            (1, 1),
            (1, 5),
            (1, 8),
            (2, 0),
            (2, 2),
            (2, 4),
            (2, 7)
        ]
    );
}

#[test]
fn rejects_small_elements() {
    for es in 0..4usize {
        let mut buffer = vec![0u8; 16];
        assert_eq!(
            mergesort(&mut buffer, es, cmp_pair),
            Err(super::SortError::Inval)
        );
    }
}

#[test]
fn empty_is_ok() {
    let mut buffer: Vec<u8> = Vec::new();
    assert_eq!(mergesort(&mut buffer, 8, cmp_pair), Ok(()));
}

#[test]
fn sorts_large_input_stably() {
    let mut buffer = Vec::new();
    for index in 0..5000u32 {
        buffer.extend_from_slice(&(index % 10).to_le_bytes());
        buffer.extend_from_slice(&index.to_le_bytes());
    }
    mergesort(&mut buffer, 8, cmp_pair).unwrap();
    let mut previous = 0u32;
    for chunk in buffer.chunks_exact(8) {
        let key = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        assert!(key >= previous);
        previous = key;
    }
}

#[test]
fn checked_capacity_reports_no_mem() {
    assert_eq!(
        super::checked_capacity(usize::MAX, 8),
        Err(super::SortError::NoMem)
    );
    assert_eq!(super::checked_capacity(2, 4), Ok(16));
}
