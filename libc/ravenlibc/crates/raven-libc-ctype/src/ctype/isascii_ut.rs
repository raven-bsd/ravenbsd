// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(clippy::arithmetic_side_effects)]
use super::*;

#[test]
fn ascii_is_non_negative_seven_bit() {
    assert!(isascii(0));
    assert!(isascii(0x7f));
    assert!(!isascii(0x80));
    assert!(!isascii(-1));
}
