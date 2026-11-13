// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn ascii_hexadecimal_only() {
    for c in b"0123456789abcdefABCDEF" {
        assert!(iswxdigit(u32::from(*c)));
    }
    assert!(!iswxdigit(u32::from(b'g')));
    assert!(!iswxdigit(0xff));
}
