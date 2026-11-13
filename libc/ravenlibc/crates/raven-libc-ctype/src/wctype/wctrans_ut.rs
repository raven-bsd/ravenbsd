// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn two_transformation_names() {
    assert_eq!(wctrans(b"toupper"), 1);
    assert_eq!(wctrans(b"tolower"), 2);
    assert_eq!(wctrans(b"upper"), 0);
    assert_eq!(towctrans(u32::from(b'a'), 1), u32::from(b'A'));
    assert_eq!(towctrans(u32::from(b'A'), 2), u32::from(b'a'));
    assert_eq!(towctrans(0x10ffff, 0), 0x10ffff);
    assert_eq!(towctrans(0x10ffff, 7), 0x10ffff);
}
