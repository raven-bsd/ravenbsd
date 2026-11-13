// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::rfind;

#[test]
fn empty_buffer() {
    assert_eq!(rfind(b"", 0), None);
    assert_eq!(rfind(b"", b'a'), None);
}

#[test]
fn last_occurrence() {
    assert_eq!(rfind(b"abcabc", b'a'), Some(3));
    assert_eq!(rfind(b"abcabc", b'c'), Some(5));
    assert_eq!(rfind(b"abcabc", b'z'), None);
    assert_eq!(rfind(b"a", b'a'), Some(0));
}

#[test]
fn every_byte_and_length() {
    for length in 0usize..=24 {
        let bytes: Vec<u8> = (0..length)
            .map(|index| (index as u8).wrapping_mul(13))
            .collect();
        for value in 0u8..=255 {
            assert_eq!(
                rfind(&bytes, value),
                bytes.iter().rposition(|&byte| byte == value)
            );
        }
    }
}

#[test]
fn high_bytes() {
    let bytes = [0x00, 0x80, 0xff, 0x80, 0x7f];
    assert_eq!(rfind(&bytes, 0x80), Some(3));
    assert_eq!(rfind(&bytes, 0xff), Some(2));
    assert_eq!(rfind(&bytes, 0x00), Some(0));
}
