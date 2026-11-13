// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(clippy::arithmetic_side_effects)]
use super::*;

#[test]
fn graphical_range() {
    assert!(isgraph(0x21));
    assert!(isgraph(0x7e));
    assert!(!isgraph(0x20));
    assert!(!isgraph(0x7f));
}
