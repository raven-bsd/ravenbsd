// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn printable_and_control_ranges() {
    assert!(iswprint(0x20));
    assert!(iswprint(0x7e));
    assert!(!iswprint(0x1f));
    assert!(!iswprint(0x7f));
    assert!(!iswprint(0x80));
    assert!(!iswprint(0x2028));
    assert!(!iswprint(0xfff9));
    assert!(!iswprint(0xd800));
    assert!(!iswprint(0xfffe));
    assert!(!iswprint(0x110000));
    assert!(!iswprint(0x10ffff));
}
