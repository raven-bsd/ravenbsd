// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn four_control_ranges() {
    assert!(iswcntrl(0));
    assert!(iswcntrl(0x1f));
    assert!(iswcntrl(0x7f));
    assert!(iswcntrl(0x9f));
    assert!(iswcntrl(0x2028));
    assert!(iswcntrl(0xfff9));
    assert!(!iswcntrl(0x20));
    assert!(!iswcntrl(0x202a));
    assert!(!iswcntrl(0xfffc));
}
