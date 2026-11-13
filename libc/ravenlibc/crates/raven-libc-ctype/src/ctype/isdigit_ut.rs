// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(clippy::arithmetic_side_effects)]
use super::*;

#[test]
fn decimal_digits_only() {
    assert!(isdigit(i32::from(b'0')));
    assert!(isdigit(i32::from(b'9')));
    assert!(!isdigit(i32::from(b'a')));
    assert!(!isdigit(-1));
    assert!(!isdigit(0x660));
}
