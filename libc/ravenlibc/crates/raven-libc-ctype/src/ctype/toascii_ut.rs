// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(clippy::arithmetic_side_effects)]
use super::*;

#[test]
fn masks_to_seven_bits() {
    assert_eq!(toascii(0x41), 0x41);
    assert_eq!(toascii(0xc1), 0x41);
    assert_eq!(toascii(-1), 0x7f);
}
