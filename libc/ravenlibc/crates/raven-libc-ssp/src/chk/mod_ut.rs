// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn chk_fail_is_a_copy_value() {
    let value = ChkFail;
    let copy = value;
    assert_eq!(value, copy);
}

#[test]
fn strlen_stops_at_nul_and_at_the_slice_end() {
    assert_eq!(strlen(b"abc\0def"), 3);
    assert_eq!(strlen(b"abc"), 3);
    assert_eq!(strlen(b""), 0);
    assert_eq!(strlen(b"\0"), 0);
}
