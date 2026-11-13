// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::ccopy;

#[test]
fn stops_after_the_byte() {
    let mut dst = [0xAAu8; 8];
    assert_eq!(ccopy(&mut dst, b"abcdef", b'c'), Some(3));
    assert_eq!(&dst[..3], b"abc");
    assert_eq!(dst[3], 0xAA);
}

#[test]
fn absent_byte_copies_everything() {
    let mut dst = [0xAAu8; 8];
    assert_eq!(ccopy(&mut dst, b"abcdef", b'z'), None);
    assert_eq!(&dst[..6], b"abcdef");
    assert_eq!(dst[6], 0xAA);
}

#[test]
fn zero_length() {
    let mut dst = [0xAAu8; 4];
    assert_eq!(ccopy(&mut dst, b"", b'a'), None);
    assert_eq!(dst, [0xAA; 4]);
    assert_eq!(ccopy(&mut [], b"abc", b'a'), None);
}

#[test]
fn n_bounds_both_sides() {
    let mut dst = [0xAAu8; 2];
    assert_eq!(ccopy(&mut dst, b"abcdef", b'c'), None);
    assert_eq!(dst, [b'a', b'b']);
    let mut dst = [0xAAu8; 8];
    assert_eq!(ccopy(&mut dst, b"ab", b'b'), Some(2));
    assert_eq!(&dst[..2], b"ab");
}

#[test]
fn high_byte_value() {
    let mut dst = [0u8; 4];
    assert_eq!(ccopy(&mut dst, &[0x7f, 0x80, 0xff], 0x80), Some(2));
    assert_eq!(dst, [0x7f, 0x80, 0, 0]);
}

#[test]
fn all_bytes_and_lengths() {
    for length in 0usize..=16 {
        let src: Vec<u8> = (0..length)
            .map(|index| (index as u8).wrapping_mul(11))
            .collect();
        for value in 0u8..=255 {
            let mut dst = vec![0x5Au8; length + 3];
            let returned = ccopy(&mut dst, &src, value);
            let expected = src
                .iter()
                .position(|&byte| byte == value)
                .map(|index| index + 1);
            assert_eq!(returned, expected, "len {length} value {value}");
            for index in 0..length {
                let expected_byte = if let Some(stop) = expected {
                    if index < stop { src[index] } else { 0x5A }
                } else {
                    src[index]
                };
                assert_eq!(
                    dst[index], expected_byte,
                    "len {length} value {value} index {index}"
                );
            }
        }
    }
}
