// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(clippy::arithmetic_side_effects)]
use super::*;

#[test]
fn hexadecimal_sets() {
    for c in b"0123456789abcdefABCDEF" {
        assert!(isxdigit(i32::from(*c)));
    }
    assert!(!isxdigit(i32::from(b'g')));
    assert!(!isxdigit(0xff));
}
