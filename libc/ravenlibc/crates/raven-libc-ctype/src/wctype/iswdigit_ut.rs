// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn ascii_digits_only() {
    assert!(iswdigit(u32::from(b'0')));
    assert!(iswdigit(u32::from(b'9')));
    assert!(!iswdigit(0x660));
    assert!(!iswdigit(u32::from(b'a')));
}
