// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(clippy::arithmetic_side_effects)]
use super::*;

#[test]
fn lower_case_range() {
    assert!(islower(i32::from(b'a')));
    assert!(islower(i32::from(b'z')));
    assert!(!islower(i32::from(b'A')));
    assert!(!islower(0xe9));
}
