// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for `getauxval`.

use super::*;
use crate::consts::AT_PAGESZ;

#[test]
fn a_present_key_returns_its_value() {
    let mut table = AuxTable::new();
    table.insert(AT_PAGESZ, 4096);
    assert_eq!(getauxval(&table, AT_PAGESZ, false), Ok(4096));
}

#[test]
fn an_absent_key_is_enoent() {
    let table = AuxTable::new();
    assert_eq!(getauxval(&table, AT_PAGESZ, false), Err(ENOENT));
}

#[test]
fn at_secure_uses_the_libc_flag() {
    let mut table = AuxTable::new();
    table.insert(AT_SECURE, 1);
    assert_eq!(getauxval(&table, AT_SECURE, false), Ok(0));
    assert_eq!(getauxval(&table, AT_SECURE, true), Ok(1));
}

#[test]
fn a_zero_value_is_returned() {
    let mut table = AuxTable::new();
    table.insert(0x1234, 0);
    assert_eq!(getauxval(&table, 0x1234, false), Ok(0));
}
