// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(clippy::arithmetic_side_effects)]
use super::*;

#[test]
fn letters_only() {
    assert!(isalpha(i32::from(b'a')));
    assert!(isalpha(i32::from(b'Z')));
    assert!(!isalpha(i32::from(b'0')));
    assert!(!isalpha(-23));
    assert!(!isalpha(i32::MIN));
    assert!(!isalpha(i32::MAX));
}
