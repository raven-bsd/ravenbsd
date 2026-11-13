// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for `getprogname`.

use super::*;

#[test]
fn the_stored_name_is_returned() {
    assert_eq!(getprogname(b"sh"), b"sh");
    assert_eq!(getprogname(b""), b"");
    assert_eq!(getprogname(&[0xff, 0xfe]), &[0xff, 0xfe]);
}
