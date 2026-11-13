// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn simple_case_mappings() {
    assert_eq!(towupper(u32::from(b'a')), u32::from(b'A'));
    assert_eq!(towlower(u32::from(b'A')), u32::from(b'a'));
    assert_eq!(towupper(u32::from(b'A')), u32::from(b'A'));
    assert_eq!(towupper(0x10ffff), 0x10ffff);
    assert_eq!(towlower(0x10ffff), 0x10ffff);
}
