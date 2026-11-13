// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::{compare, find};

#[test]
fn compare_equal() {
    assert_eq!(compare(b"", b""), 0);
    assert_eq!(compare(b"abc", b"abc"), 0);
    assert_eq!(compare(b"abc", b"abd"), -1);
    assert_eq!(compare(b"abd", b"abc"), 1);
}

#[test]
fn compare_is_unsigned() {
    assert_eq!(compare(&[0x7f], &[0x80]), -1);
    assert_eq!(compare(&[0xff], &[0x00]), 255);
    assert_eq!(compare(&[0x80], &[0x7f]), 1);
    assert_eq!(compare(&[0x00], &[0xff]), -255);
}

#[test]
fn compare_common_prefix_only() {
    assert_eq!(compare(b"abc", b"abcdef"), 0);
    assert_eq!(compare(b"abcdef", b"abc"), 0);
}

#[test]
fn find_every_byte() {
    for length in 0usize..=32 {
        let bytes: Vec<u8> = (0..length)
            .map(|index| (index as u8).wrapping_mul(7))
            .collect();
        for value in 0u8..=255 {
            assert_eq!(
                find(&bytes, value),
                bytes.iter().position(|&byte| byte == value)
            );
        }
    }
}

#[test]
fn find_empty() {
    assert_eq!(find(b"", 0), None);
    assert_eq!(find(b"", 255), None);
}

#[test]
fn find_high_bytes() {
    let bytes = [0x7f, 0x80, 0xff, 0x00, 0x80];
    assert_eq!(find(&bytes, 0x80), Some(1));
    assert_eq!(find(&bytes, 0xff), Some(2));
    assert_eq!(find(&bytes, 0x00), Some(3));
    assert_eq!(find(&bytes, 0x81), None);
}
