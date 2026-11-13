// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn signed_misses_high_bytes() {
    assert_eq!(signed(&[0x80], 0x80), None);
    assert_eq!(signed(b"abc", i32::from(b'b')), Some(1));
}

#[test]
fn lastbyte_never_matches_the_last_byte() {
    assert_eq!(lastbyte(b"abc", i32::from(b'c')), None);
    assert_eq!(lastbyte(b"abc", i32::from(b'a')), Some(0));
}
