// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::*;

#[test]
fn byte_at_end_is_nul() {
    assert_eq!(byte_at(b"abc", 0), b'a');
    assert_eq!(byte_at(b"abc", 2), b'c');
    assert_eq!(byte_at(b"abc", 3), 0);
    assert_eq!(byte_at(b"abc", 100), 0);
    assert_eq!(byte_at(b"", 0), 0);
}

#[test]
fn strlcpy_worked_example() {
    let mut buf = [0xAAu8; 8];
    assert_eq!(strlcpy(&mut buf, b"abc"), 3);
    assert_eq!(&buf[..4], b"abc\0");
    assert_eq!(buf[4], 0xAA);
}

#[test]
fn strlcpy_truncates_and_terminates() {
    let mut buf = [0xAAu8; 4];
    assert_eq!(strlcpy(&mut buf, b"abcdefgh"), 8);
    assert_eq!(&buf, b"abc\0");
}

#[test]
fn strlcpy_size_one_writes_only_nul() {
    let mut buf = [0xAAu8; 1];
    assert_eq!(strlcpy(&mut buf, b"abc"), 3);
    assert_eq!(buf[0], 0);
}

#[test]
fn strlcpy_size_zero_writes_nothing() {
    let mut buf = [0u8; 0];
    assert_eq!(strlcpy(&mut buf, b"abc"), 3);
}

#[test]
fn strlcpy_empty_source() {
    let mut buf = [0xAAu8; 5];
    assert_eq!(strlcpy(&mut buf, b""), 0);
    assert_eq!(buf[0], 0);
    assert_eq!(buf[1], 0xAA);
}

#[test]
fn strlcat_returns_capped_dst_plus_src() {
    let mut buf = [0xAAu8; 8];
    buf[..4].copy_from_slice(b"abc\0");
    assert_eq!(strlcat(&mut buf, b"de"), 5);
    assert_eq!(&buf[..6], b"abcde\0");
    assert_eq!(buf[6], 0xAA);
}

#[test]
fn strlcat_without_nul_leaves_dst_untouched() {
    let mut buf = *b"abcdefgh";
    assert_eq!(strlcat(&mut buf, b"xy"), 10);
    assert_eq!(&buf, b"abcdefgh");
}

#[test]
fn strlcat_size_zero() {
    let mut buf = [0u8; 0];
    assert_eq!(strlcat(&mut buf, b"abc"), 3);
}

#[test]
fn strlcat_truncates() {
    let mut buf = [0xAAu8; 5];
    buf[..4].copy_from_slice(b"abc\0");
    assert_eq!(strlcat(&mut buf, b"defgh"), 8);
    assert_eq!(&buf, b"abcd\0");
}
