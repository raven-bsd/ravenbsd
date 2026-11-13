// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::assertions_on_constants
)]

//! `setenv` overwrite rules and errors.

use super::*;
use crate::env::table::EnvTable;

#[test]
fn insert_and_overwrite() {
    let mut env = EnvTable::new();
    setenv(&mut env, b"A", b"1", true).unwrap();
    assert_eq!(env.getenv(b"A"), Some(b"1".as_slice()));
    setenv(&mut env, b"A", b"2", false).unwrap();
    assert_eq!(env.getenv(b"A"), Some(b"1".as_slice()));
    setenv(&mut env, b"A", b"2", true).unwrap();
    assert_eq!(env.getenv(b"A"), Some(b"2".as_slice()));
}

#[test]
fn rejects_invalid_names() {
    let mut env = EnvTable::new();
    assert!(setenv(&mut env, b"", b"1", true).is_err());
    assert!(setenv(&mut env, b"A=B", b"1", true).is_err());
    assert!(env.is_empty());
}

#[test]
fn old_value_pointer_stays_valid() {
    let mut env = EnvTable::new();
    setenv(&mut env, b"A", b"first", true).unwrap();
    let old = env.getenv(b"A").unwrap();
    setenv(&mut env, b"A", b"second", true).unwrap();
    assert_eq!(old, b"first");
    assert_eq!(env.getenv(b"A"), Some(b"second".as_slice()));
}

#[test]
fn empty_value_is_valid() {
    let mut env = EnvTable::new();
    setenv(&mut env, b"A", b"", true).unwrap();
    assert_eq!(env.getenv(b"A"), Some(b"".as_slice()));
}
