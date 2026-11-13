// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

//! Collation tests.

use super::*;

#[test]
fn strcoll_is_unsigned_byte_order() {
    assert_eq!(strcoll(b"abc", b"abc"), 0);
    assert!(strcoll(b"abc", b"abd") < 0);
    assert!(strcoll(b"a", b"ab") < 0);
    assert!(strcoll(&[0xff], &[0x01]) > 0);
}

#[test]
fn strxfrm_copies_only_when_the_buffer_is_longer() {
    let mut buffer = [0u8; 4];
    assert_eq!(strxfrm(Some(&mut buffer), b"abc"), 3);
    assert_eq!(&buffer[..3], b"abc");
    let mut small = [0xaa; 2];
    assert_eq!(strxfrm(Some(&mut small), b"abc"), 3);
    assert_eq!(small, [0xaa; 2]);
    assert_eq!(strxfrm(None, b"abc"), 3);
}

#[test]
fn wcscoll_and_wcsxfrm_use_unsigned_code_units() {
    assert_eq!(wcscoll(&[1, 2], &[1, 2]), 0);
    assert!(wcscoll(&[0x8000_0000], &[1]) > 0);
    let mut buffer = [0u32; 4];
    assert_eq!(wcsxfrm(Some(&mut buffer), &[1, 2, 3]), 3);
    assert_eq!(buffer, [1, 2, 3, 0]);
    let mut small = [9u32; 3];
    assert_eq!(wcsxfrm(Some(&mut small), &[1, 2, 3]), 3);
    assert_eq!(small, [1, 2, 0]);
}
