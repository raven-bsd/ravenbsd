// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `ptsname_r` name formatting.

use super::*;

#[test]
fn names_pair_zero() {
    let mut buffer = [0u8; 32];
    let length = slave_path(0, &mut buffer).unwrap_or(usize::MAX);
    assert_eq!(&buffer[..length], b"/dev/pts/0");
    assert_eq!(buffer[length], 0);
}

#[test]
fn names_a_multi_digit_pair() {
    let mut buffer = [0u8; 32];
    let length = slave_path(1234, &mut buffer).unwrap_or(usize::MAX);
    assert_eq!(&buffer[..length], b"/dev/pts/1234");
    assert_eq!(buffer[length], 0);
}

#[test]
fn rejects_a_short_buffer() {
    let mut buffer = [0u8; 8];
    assert_eq!(slave_path(1, &mut buffer), Err(ERANGE as u16));
}
