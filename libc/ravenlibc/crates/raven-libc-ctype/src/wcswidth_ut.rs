// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn empty_and_nul_are_zero() {
    assert_eq!(wcswidth(&[]), 0);
    assert_eq!(wcswidth(&[0]), 0);
    assert_eq!(wcswidth(&[0x41, 0]), 1);
}

#[test]
fn sums_and_propagates_minus_one() {
    assert_eq!(wcswidth(&[0x41, 0x42, 0x4e00]), 4);
    assert_eq!(wcswidth(&[0x41, 0x0a, 0x42]), -1);
}
