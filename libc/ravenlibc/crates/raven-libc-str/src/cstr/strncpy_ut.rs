// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::strncpy;

#[test]
fn pads_with_nul() {
    let mut dst = [0xAAu8; 8];
    strncpy(&mut dst, b"abc");
    assert_eq!(&dst[..4], b"abc\0");
    assert_eq!(&dst[4..], &[0, 0, 0, 0]);
}

#[test]
fn no_terminator_when_source_is_long() {
    let mut dst = [0xAAu8; 4];
    strncpy(&mut dst, b"abcdef");
    assert_eq!(&dst, b"abcd");
}

#[test]
fn exact_source_length_writes_no_nul() {
    let mut dst = [0xAAu8; 3];
    strncpy(&mut dst, b"abc");
    assert_eq!(&dst, b"abc");
}

#[test]
fn zero_length_destination() {
    let mut dst = [0u8; 0];
    strncpy(&mut dst, b"abc");
}

#[test]
fn embedded_nul_pads() {
    let mut dst = [0xAAu8; 5];
    strncpy(&mut dst, b"a\0bc");
    assert_eq!(&dst, b"a\0\0\0\0");
}

#[test]
fn every_length() {
    for src_len in 0usize..=8 {
        for dst_len in 0usize..=8 {
            let src: Vec<u8> = (0..src_len).map(|index| b'a' + index as u8).collect();
            let mut dst = vec![0xAAu8; dst_len];
            strncpy(&mut dst, &src);
            for index in 0..dst_len {
                let expected = if index < src_len { src[index] } else { 0 };
                assert_eq!(
                    dst[index], expected,
                    "src {src_len} dst {dst_len} index {index}"
                );
            }
        }
    }
}
