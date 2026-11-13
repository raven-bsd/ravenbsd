// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn an_empty_string_is_zero() {
    assert_eq!(strnlen(b"", 0), 0);
    assert_eq!(strnlen(b"", 10), 0);
}

#[test]
fn the_terminator_is_not_counted() {
    assert_eq!(strnlen(b"abc\0", 10), 3);
    assert_eq!(strnlen(b"\0", 10), 0);
}

#[test]
fn maxlen_caps_the_scan() {
    assert_eq!(strnlen(b"abcdef", 3), 3);
    assert_eq!(strnlen(b"abc\0def", 3), 3);
    assert_eq!(strnlen(b"abc\0def", 4), 3);
}

#[test]
fn an_embedded_nul_terminates_early() {
    assert_eq!(strnlen(b"ab\0cd", 10), 2);
    assert_eq!(strnlen(b"\0abc", 10), 0);
}
