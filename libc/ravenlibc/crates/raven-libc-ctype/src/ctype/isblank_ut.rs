// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(clippy::arithmetic_side_effects)]
use super::*;

#[test]
fn blank_is_space_or_tab() {
    assert!(isblank(0x20));
    assert!(isblank(0x09));
    assert!(!isblank(0x0a));
    assert!(!isblank(0xa0));
}
