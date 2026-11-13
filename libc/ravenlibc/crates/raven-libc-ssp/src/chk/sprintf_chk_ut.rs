// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn result_below_the_buffer_is_ok() {
    assert_eq!(sprintf_chk(8, 16), Ok(()));
}

#[test]
fn result_equal_to_the_buffer_fails() {
    assert_eq!(sprintf_chk(16, 16), Err(ChkFail));
}

#[test]
fn negative_result_is_ignored() {
    assert_eq!(sprintf_chk(-1, 16), Ok(()));
}

#[test]
fn huge_object_size_skips_the_check() {
    assert_eq!(sprintf_chk(i32::MAX, i32::MAX as usize + 1), Ok(()));
}
