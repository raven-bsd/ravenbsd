// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(clippy::arithmetic_side_effects)]
use super::*;

#[test]
fn maps_lower_only() {
    assert_eq!(toupper(i32::from(b'a')), i32::from(b'A'));
    assert_eq!(toupper(i32::from(b'A')), i32::from(b'A'));
    assert_eq!(toupper(0xe9), 0xe9);
    assert_eq!(toupper(-23), -23);
}
