// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(clippy::arithmetic_side_effects)]
use super::*;

#[test]
fn upper_case_range() {
    assert!(isupper(i32::from(b'A')));
    assert!(isupper(i32::from(b'Z')));
    assert!(!isupper(i32::from(b'a')));
    assert!(!isupper(0xc9));
}
