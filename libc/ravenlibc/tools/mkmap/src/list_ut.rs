// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the small file helpers.

use super::*;
use crate::testutil;

#[test]
fn write_then_read_round_trips() {
    let dir = testutil::tempdir("list");
    let path = dir.path.join("nested/Symbols.list");
    write(&path, "a\n").expect("write");
    assert_eq!(read(&path).expect("read"), "a\n");
}

#[test]
fn read_of_a_missing_file_is_an_error() {
    let dir = testutil::tempdir("list-missing");
    assert!(read(&dir.path.join("absent")).is_err());
}
