// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn exact_fit_returns_source_length() {
    assert_eq!(strlcpy_chk(b"abc", 4, 4), Ok(3));
}

#[test]
fn destination_larger_than_object_fails() {
    assert_eq!(strlcpy_chk(b"abc", 5, 4), Err(ChkFail));
}

#[test]
fn truncation_is_allowed() {
    assert_eq!(strlcpy_chk(b"abcdef", 3, 3), Ok(6));
}

#[test]
fn unknown_size_never_fails() {
    assert_eq!(strlcpy_chk(b"abc", 4, usize::MAX), Ok(3));
}
