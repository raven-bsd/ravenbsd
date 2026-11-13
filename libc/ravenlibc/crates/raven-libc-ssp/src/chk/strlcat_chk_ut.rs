// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn append_returns_the_combined_length() {
    assert_eq!(strlcat_chk(b"abc", b"de", 6, 6), Ok(5));
}

#[test]
fn destination_larger_than_object_fails() {
    assert_eq!(strlcat_chk(b"abc", b"de", 7, 6), Err(ChkFail));
}

#[test]
fn full_destination_returns_without_copying() {
    assert_eq!(strlcat_chk(b"abcdef", b"xy", 6, 6), Ok(8));
}

#[test]
fn truncation_accounts_for_the_byte_budget() {
    assert_eq!(strlcat_chk(b"abc", b"def", 5, 5), Ok(6));
}

#[test]
fn unknown_size_never_fails() {
    assert_eq!(strlcat_chk(b"abc", b"de", 6, usize::MAX), Ok(5));
}
