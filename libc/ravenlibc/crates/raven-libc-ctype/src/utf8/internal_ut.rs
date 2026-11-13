// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn oob_accepts_continuation_bytes_inside_the_allowed_range() {
    assert!(!oob(0xc000_0002, 0x80));
    assert!(!oob(0xc000_0002, 0xbf));
    assert!(oob(0xc000_0002, 0x7f));
    assert!(oob(0xc000_0002, 0xc0));
}

#[test]
fn codeunit_is_the_byte_in_the_surrogate_range() {
    assert_eq!(codeunit(0x41), 0x41);
    assert_eq!(codeunit(0x80), 0xdf80);
    assert_eq!(codeunit(0xff), 0xdfff);
    assert!(is_codeunit(0xdf80));
    assert!(is_codeunit(0xdfff));
    assert!(!is_codeunit(0xdf7f));
    assert!(!is_codeunit(0xe000));
    assert_eq!(SA, 0xc2);
    assert_eq!(SB, 0xf4);
}
