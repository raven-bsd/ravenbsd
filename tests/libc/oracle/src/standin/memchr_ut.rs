// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn a_missing_byte_is_none() {
    assert_eq!(memchr(b"abc", i32::from(b'z')), None);
    assert_eq!(memchr(b"", 0), None);
}

#[test]
fn the_first_match_wins() {
    assert_eq!(memchr(b"abca", i32::from(b'a')), Some(0));
    assert_eq!(memchr(b"abca", i32::from(b'c')), Some(2));
}

#[test]
fn the_search_byte_is_truncated_to_a_byte() {
    assert_eq!(memchr(b"abc", 0x161), Some(0));
    assert_eq!(memchr(b"abc", -1), None);
}

#[test]
fn high_bytes_match_exactly() {
    assert_eq!(memchr(&[0x00, 0x7F, 0x80, 0xFF], 0x80), Some(2));
    assert_eq!(memchr(&[0x00, 0x7F, 0x80, 0xFF], 0xFF), Some(3));
}
