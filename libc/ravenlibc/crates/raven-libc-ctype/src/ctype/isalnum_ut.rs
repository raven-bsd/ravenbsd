// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(clippy::arithmetic_side_effects)]
use super::*;

#[test]
fn letters_and_digits() {
    assert!(isalnum(i32::from(b'a')));
    assert!(isalnum(i32::from(b'7')));
    assert!(!isalnum(i32::from(b'_')));
    assert!(!isalnum(-1));
}
