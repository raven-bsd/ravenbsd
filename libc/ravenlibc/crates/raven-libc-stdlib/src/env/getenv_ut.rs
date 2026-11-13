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

//! `getenv` and `getenv_r` vectors.

use super::*;
use crate::env::table::EnvTable;

fn leak(bytes: &[u8]) -> &'static [u8] {
    Box::leak(bytes.to_vec().into_boxed_slice())
}

fn table() -> EnvTable {
    let mut env = EnvTable::new();
    env.adopt_initial(&[leak(b"A=1\0"), leak(b"B=two\0"), leak(b"EMPTY=\0")]);
    env
}

#[test]
fn getenv_returns_the_value_without_nul() {
    let env = table();
    assert_eq!(getenv(&env, b"A"), Some(b"1".as_slice()));
    assert_eq!(getenv(&env, b"B"), Some(b"two".as_slice()));
    assert_eq!(getenv(&env, b"EMPTY"), Some(b"".as_slice()));
    assert_eq!(getenv(&env, b"C"), None);
}

#[test]
fn getenv_ignores_names_with_equals() {
    let env = table();
    assert_eq!(getenv(&env, b"A=1"), None);
    assert_eq!(getenv(&env, b""), None);
}

#[test]
fn getenv_r_copies_and_nul_terminates() {
    let env = table();
    let mut buf = [0xeeu8; 8];
    assert_eq!(getenv_r(&env, b"B", &mut buf), Ok(3));
    assert_eq!(&buf[..4], b"two\0");
    assert_eq!(buf[4], 0xee);
}

#[test]
fn getenv_r_erange() {
    let env = table();
    let mut buf = [0u8; 2];
    assert_eq!(
        getenv_r(&env, b"B", &mut buf),
        Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::ERANGE as u16
        ))
    );
    let mut one = [0u8; 1];
    assert_eq!(getenv_r(&env, b"EMPTY", &mut one), Ok(0));
    assert_eq!(one[0], 0);
}

#[test]
fn getenv_r_missing_is_enoent() {
    let env = table();
    let mut buf = [0u8; 8];
    assert_eq!(
        getenv_r(&env, b"ZZ", &mut buf),
        Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::ENOENT as u16
        ))
    );
}
