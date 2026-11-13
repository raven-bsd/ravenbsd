// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn exact_fit_is_ok() {
    assert_eq!(fgets_chk(16, 16), Ok(()));
}

#[test]
fn one_over_fails() {
    assert_eq!(fgets_chk(17, 16), Err(ChkFail));
}

#[test]
fn negative_length_is_ok() {
    assert_eq!(fgets_chk(-1, 0), Ok(()));
}

#[test]
fn huge_object_size_skips_the_check() {
    assert_eq!(fgets_chk(i32::MAX, i32::MAX as usize), Ok(()));
}
