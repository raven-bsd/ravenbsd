// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(clippy::arithmetic_side_effects)]
use super::*;

#[test]
fn control_range() {
    assert!(iscntrl(0));
    assert!(iscntrl(0x1f));
    assert!(iscntrl(0x7f));
    assert!(!iscntrl(0x20));
    assert!(!iscntrl(0x80));
}
