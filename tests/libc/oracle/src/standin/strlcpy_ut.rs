// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn an_empty_source_writes_only_the_terminator() {
    let mut buf = [0xAA; 4];
    assert_eq!(strlcpy(&mut buf, 4, b""), 0);
    assert_eq!(buf, [0, 0xAA, 0xAA, 0xAA]);
}

#[test]
fn a_fitting_source_is_copied_and_terminated() {
    let mut buf = [0xAA; 8];
    assert_eq!(strlcpy(&mut buf, 8, b"hello"), 5);
    assert_eq!(&buf[..6], b"hello\0");
    assert_eq!(&buf[6..], [0xAA, 0xAA]);
}

#[test]
fn a_truncated_source_keeps_the_guard_bytes() {
    let mut buf = [0xAA; 7];
    assert_eq!(strlcpy(&mut buf, 4, b"hello"), 5);
    assert_eq!(&buf[..4], b"hel\0");
    assert_eq!(&buf[4..], [0xAA, 0xAA, 0xAA]);
}

#[test]
fn dsize_zero_writes_nothing() {
    let mut buf = [0xAA; 4];
    assert_eq!(strlcpy(&mut buf, 0, b"hello"), 5);
    assert_eq!(buf, [0xAA; 4]);
}

#[test]
fn an_exact_fit_terminates_at_the_last_byte() {
    let mut buf = [0xAA; 6];
    assert_eq!(strlcpy(&mut buf, 6, b"hello"), 5);
    assert_eq!(buf, *b"hello\0");
}
