// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `getauxval` presence rules.

use super::*;
use crate::seams::Errno;

#[test]
fn present_key_returns_value_and_zero_errno() {
    assert_eq!(getauxval(6, &|key| (key == 6).then_some(4096)), (4096, 0));
}

#[test]
fn absent_key_returns_zero_and_enoent() {
    assert_eq!(getauxval(6, &|_| None), (0, i32::from(Errno::ENOENT.0)));
}

#[test]
fn present_zero_value_is_not_an_error() {
    let (value, error) = getauxval(23, &|key| (key == 23).then_some(0));
    assert_eq!(value, 0);
    assert_eq!(error, 0);
}

#[test]
fn lookup_key_is_passed_through() {
    assert_eq!(getauxval(0x7007, &|key| Some(key)), (0x7007, 0));
}

#[test]
fn at_null_is_zero() {
    assert_eq!(AT_NULL, 0);
}
