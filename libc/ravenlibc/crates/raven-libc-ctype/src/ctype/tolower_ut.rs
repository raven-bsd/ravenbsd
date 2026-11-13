// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(clippy::arithmetic_side_effects)]
use super::*;

#[test]
fn maps_upper_only() {
    assert_eq!(tolower(i32::from(b'A')), i32::from(b'a'));
    assert_eq!(tolower(i32::from(b'a')), i32::from(b'a'));
    assert_eq!(tolower(0xc9), 0xc9);
    assert_eq!(tolower(-1), -1);
}
