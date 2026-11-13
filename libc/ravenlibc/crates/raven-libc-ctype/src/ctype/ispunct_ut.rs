// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(clippy::arithmetic_side_effects)]
use super::*;

#[test]
fn punctuation_excludes_alnum_and_space() {
    for &c in b"!/:@[~" {
        assert!(ispunct(i32::from(c)));
    }
    assert!(!ispunct(i32::from(b'a')));
    assert!(!ispunct(i32::from(b' ')));
    assert!(!ispunct(0x80));
}
