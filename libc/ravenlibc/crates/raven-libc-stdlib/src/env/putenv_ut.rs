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

//! `putenv`: keeps the caller's slice, and `"NAME"` removes (RD-26).

use super::*;
use crate::env::table::EnvTable;

fn leak(bytes: &[u8]) -> &'static [u8] {
    Box::leak(bytes.to_vec().into_boxed_slice())
}

#[test]
fn stores_the_callers_slice() {
    let mut env = EnvTable::new();
    let text = leak(b"A=value\0");
    putenv(&mut env, text).unwrap();
    assert_eq!(env.getenv(b"A"), Some(b"value".as_slice()));
    assert!(env.as_slice().contains(&text));
}

#[test]
fn replaces_an_existing_entry() {
    let mut env = EnvTable::new();
    putenv(&mut env, leak(b"A=one\0")).unwrap();
    putenv(&mut env, leak(b"A=two\0")).unwrap();
    assert_eq!(env.getenv(b"A"), Some(b"two".as_slice()));
    assert_eq!(env.len(), 1);
}

#[test]
fn name_without_equal_removes() {
    let mut env = EnvTable::new();
    putenv(&mut env, leak(b"A=one\0")).unwrap();
    putenv(&mut env, leak(b"A\0")).unwrap();
    assert_eq!(env.getenv(b"A"), None);
    putenv(&mut env, leak(b"A")).unwrap();
    assert_eq!(env.getenv(b"A"), None);
}

#[test]
fn malformed_names_are_einval() {
    let mut env = EnvTable::new();
    assert!(putenv(&mut env, leak(b"=value\0")).is_err());
    assert!(putenv(&mut env, leak(b"\0")).is_err());
}

#[test]
fn empty_value_is_stored() {
    let mut env = EnvTable::new();
    putenv(&mut env, leak(b"A=\0")).unwrap();
    assert_eq!(env.getenv(b"A"), Some(b"".as_slice()));
}
