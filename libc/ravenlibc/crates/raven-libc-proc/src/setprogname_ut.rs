// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for `setprogname`.

use super::*;

#[test]
fn a_plain_name_is_unchanged() {
    assert_eq!(setprogname(b"sh"), b"sh");
}

#[test]
fn the_basename_is_returned() {
    assert_eq!(setprogname(b"/bin/sh"), b"sh");
    assert_eq!(setprogname(b"x/y/z"), b"z");
}

#[test]
fn a_trailing_slash_gives_an_empty_name() {
    assert_eq!(setprogname(b"a/"), b"");
    assert_eq!(setprogname(b"/"), b"");
}

#[test]
fn non_utf8_bytes_are_kept() {
    assert_eq!(setprogname(&[0xff, b'/', 0xfe]), &[0xfe]);
}
