// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn exact_fit_returns_length_with_nul() {
    assert_eq!(strcpy_chk(b"abc", 4, 0x1000, 0x2000), Ok(4));
}

#[test]
fn one_over_fails() {
    assert_eq!(strcpy_chk(b"abc", 3, 0x1000, 0x2000), Err(ChkFail));
}

#[test]
fn overlap_fails() {
    assert_eq!(strcpy_chk(b"abc", 4, 0x1000, 0x1002), Err(ChkFail));
}

#[test]
fn unknown_size_never_fails() {
    assert_eq!(strcpy_chk(b"abc", usize::MAX, 0x1000, 0x2000), Ok(4));
}
