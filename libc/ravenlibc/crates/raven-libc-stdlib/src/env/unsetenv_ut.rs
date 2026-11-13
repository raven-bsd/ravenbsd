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

//! `unsetenv` and `clearenv`.

use super::*;
use crate::env::clearenv;
use crate::env::table::EnvTable;

fn leak(bytes: &[u8]) -> &'static [u8] {
    Box::leak(bytes.to_vec().into_boxed_slice())
}

#[test]
fn removes_all_matches() {
    let mut env = EnvTable::new();
    env.adopt_initial(&[leak(b"A=1\0"), leak(b"B=2\0"), leak(b"A=3\0")]);
    unsetenv(&mut env, b"A").unwrap();
    assert_eq!(env.len(), 1);
    assert_eq!(env.getenv(b"A"), None);
    assert_eq!(env.getenv(b"B"), Some(b"2".as_slice()));
}

#[test]
fn missing_is_ok() {
    let mut env = EnvTable::new();
    assert!(unsetenv(&mut env, b"Z").is_ok());
}

#[test]
fn invalid_names() {
    let mut env = EnvTable::new();
    assert!(unsetenv(&mut env, b"").is_err());
    assert!(unsetenv(&mut env, b"A=B").is_err());
}

#[test]
fn clearenv_forgets_the_list_not_the_strings() {
    let mut env = EnvTable::new();
    let keep = leak(b"KEEP=value\0");
    env.adopt_initial(&[keep]);
    let before = env.getenv(b"KEEP").unwrap();
    clearenv(&mut env);
    assert!(env.is_empty());
    assert_eq!(before, b"value");
    assert_eq!(keep, b"KEEP=value\0");
}
