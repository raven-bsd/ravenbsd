// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn exact_fit_is_ok() {
    assert_eq!(snprintf_chk(16, 16), Ok(()));
}

#[test]
fn one_over_fails() {
    assert_eq!(snprintf_chk(17, 16), Err(ChkFail));
}

#[test]
fn unknown_size_never_fails() {
    assert_eq!(snprintf_chk(16, usize::MAX), Ok(()));
}

#[test]
fn zero_length_is_ok() {
    assert_eq!(snprintf_chk(0, 0), Ok(()));
}
