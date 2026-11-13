// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn nonul_omits_the_terminator_on_truncation() {
    let mut buf = [0xAA; 6];
    nonul(&mut buf, 4, b"hello");
    assert_eq!(&buf[..3], b"hel");
    assert_eq!(buf[3], 0xAA);
}

#[test]
fn ret_returns_the_copied_length() {
    let mut buf = [0xAA; 6];
    assert_eq!(ret(&mut buf, 4, b"hello"), 3);
}

#[test]
fn overrun_writes_past_the_destination() {
    let mut buf = [0xAA; 6];
    overrun(&mut buf, 4, b"hello");
    assert_eq!(buf[4], 0);
}

#[test]
fn zero_writes_at_offset_zero_for_dsize_zero() {
    let mut buf = [0xAA; 2];
    zero(&mut buf, 0, b"x");
    assert_eq!(buf[0], 0);
}
