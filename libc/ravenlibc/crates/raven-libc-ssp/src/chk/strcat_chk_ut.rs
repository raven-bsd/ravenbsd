// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn exact_fit_is_ok() {
    assert_eq!(strcat_chk(b"abc", b"de", 6), Ok(()));
}

#[test]
fn one_over_fails() {
    assert_eq!(strcat_chk(b"abc", b"de", 5), Err(ChkFail));
}

#[test]
fn empty_strings_need_one_byte() {
    assert_eq!(strcat_chk(b"", b"", 1), Ok(()));
    assert_eq!(strcat_chk(b"", b"", 0), Err(ChkFail));
}

#[test]
fn unknown_size_never_fails() {
    assert_eq!(strcat_chk(b"a", b"b", usize::MAX), Ok(()));
}
