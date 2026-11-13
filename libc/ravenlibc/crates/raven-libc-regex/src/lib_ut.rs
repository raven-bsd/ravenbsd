// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests of the public wrapper types.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use super::*;
use crate::consts::REG_EXTENDED;

#[test]
fn nsub_reports_the_group_count() {
    let re = regcomp(b"(a)(b)", REG_EXTENDED, &ByteLocale).expect("compile");
    assert_eq!(re.nsub(), 2);
    let plain = regcomp(b"abc", REG_EXTENDED, &ByteLocale).expect("compile");
    assert_eq!(plain.nsub(), 0);
}

#[test]
fn regex_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Regex>();
    assert_send_sync::<RegMatch>();
    assert_send_sync::<RegError>();
}

#[test]
fn clone_keeps_the_magic() {
    let re = regcomp(b"a+", REG_EXTENDED, &ByteLocale).expect("compile");
    let copy = re.clone();
    assert!(copy.magic_ok());
    assert_eq!(copy.nsub(), re.nsub());
}

#[test]
fn regmatch_default_is_unused() {
    let slot = RegMatch::default();
    assert_eq!(slot.rm_so, 0);
    assert_eq!(slot.rm_eo, 0);
}
