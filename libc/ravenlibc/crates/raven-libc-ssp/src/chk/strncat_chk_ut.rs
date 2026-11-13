// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn zero_length_returns_ok() {
    assert_eq!(strncat_chk(b"abc", b"def", 0, 0), Ok(()));
}

#[test]
fn truncation_accounts_for_the_nul() {
    assert_eq!(strncat_chk(b"abc", b"def", 2, 6), Ok(()));
}

#[test]
fn over_the_buffer_fails() {
    assert_eq!(strncat_chk(b"abc", b"def", 2, 5), Err(ChkFail));
}

#[test]
fn len_over_slen_fails() {
    assert_eq!(strncat_chk(b"", b"def", 4, 3), Err(ChkFail));
}

#[test]
fn unknown_size_never_fails() {
    assert_eq!(strncat_chk(b"a", b"b", 1, usize::MAX), Ok(()));
}
