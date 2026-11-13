// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn exact_fit_is_ok() {
    assert_eq!(memcpy_chk(16, 16, 0x1000, 0x2000), Ok(()));
}

#[test]
fn one_over_fails() {
    assert_eq!(memcpy_chk(17, 16, 0x1000, 0x2000), Err(ChkFail));
}

#[test]
fn overlap_fails() {
    assert_eq!(memcpy_chk(16, 16, 0x1000, 0x1008), Err(ChkFail));
}

#[test]
fn unknown_size_never_fails() {
    assert_eq!(memcpy_chk(16, usize::MAX, 0x1000, 0x2000), Ok(()));
}
