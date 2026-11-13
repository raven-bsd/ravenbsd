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

//! `bsearch`: the `lim`-halving loop, duplicates and the empty cases.

use super::bsearch;

fn cmp_i32(key: &[u8], elem: &[u8]) -> i32 {
    let key = i32::from_le_bytes([key[0], key[1], key[2], key[3]]);
    let elem = i32::from_le_bytes([elem[0], elem[1], elem[2], elem[3]]);
    match key.cmp(&elem) {
        core::cmp::Ordering::Less => -1,
        core::cmp::Ordering::Equal => 0,
        core::cmp::Ordering::Greater => 1,
    }
}

fn encode(values: &[i32], es: usize) -> Vec<u8> {
    let mut buffer = vec![0u8; values.len() * es];
    for (index, value) in values.iter().enumerate() {
        buffer[index * es..index * es + 4].copy_from_slice(&value.to_le_bytes());
    }
    buffer
}

#[test]
fn finds_every_present_value() {
    let values: Vec<i32> = (0..100).map(|value| value * 3).collect();
    let buffer = encode(&values, 8);
    for value in &values {
        let key = value.to_le_bytes();
        let index = bsearch(&buffer, 8, |element| cmp_i32(&key, element)).unwrap();
        assert_eq!(
            i32::from_le_bytes([
                buffer[index * 8],
                buffer[index * 8 + 1],
                buffer[index * 8 + 2],
                buffer[index * 8 + 3]
            ]),
            *value
        );
    }
}

#[test]
fn absent_and_edges() {
    let values = vec![1, 3, 5, 7, 9];
    let buffer = encode(&values, 4);
    for absent in [0i32, 2, 4, 6, 8, 10, i32::MIN, i32::MAX] {
        let key = absent.to_le_bytes();
        assert_eq!(bsearch(&buffer, 4, |element| cmp_i32(&key, element)), None);
    }
}

#[test]
fn duplicates_return_an_equal_element() {
    let values = vec![7, 7, 7, 7, 7];
    let buffer = encode(&values, 4);
    let key = 7i32.to_le_bytes();
    let index = bsearch(&buffer, 4, |element| cmp_i32(&key, element)).unwrap();
    assert!(index < values.len());
}

#[test]
fn empty_and_zero_element_size() {
    let empty: [u8; 0] = [];
    let key = 1i32.to_le_bytes();
    assert_eq!(bsearch(&empty, 4, |element| cmp_i32(&key, element)), None);
    let buffer = encode(&[1], 4);
    assert_eq!(bsearch(&buffer, 0, |_| 0), None);
}
