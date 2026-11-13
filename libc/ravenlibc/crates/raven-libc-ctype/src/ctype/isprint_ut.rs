// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(clippy::arithmetic_side_effects)]
use super::*;

#[test]
fn printable_range() {
    assert!(isprint(0x20));
    assert!(isprint(0x7e));
    assert!(!isprint(0x1f));
    assert!(!isprint(0x7f));
    assert!(!isprint(0xe9));
}
