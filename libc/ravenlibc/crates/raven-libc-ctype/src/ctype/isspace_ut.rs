// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(clippy::arithmetic_side_effects)]
use super::*;

#[test]
fn six_ascii_space_characters() {
    for c in [b' ', b'\t', b'\n', 0x0b, 0x0c, b'\r'] {
        assert!(isspace(i32::from(c)));
    }
    assert!(!isspace(0xa0));
    assert!(!isspace(0));
}
