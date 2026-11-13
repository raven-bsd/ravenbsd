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

//! `secure_getenv` with both `AT_SECURE` values.

use super::*;
use crate::env::table::EnvTable;

fn leak(bytes: &[u8]) -> &'static [u8] {
    Box::leak(bytes.to_vec().into_boxed_slice())
}

#[test]
fn at_secure_hides_everything() {
    let mut env = EnvTable::new();
    env.adopt_initial(&[leak(b"PATH=/bin\0")]);
    assert_eq!(
        secure_getenv(&env, b"PATH", false),
        Some(b"/bin".as_slice())
    );
    assert_eq!(secure_getenv(&env, b"PATH", true), None);
}

#[test]
fn missing_and_invalid_names() {
    let env = EnvTable::new();
    assert_eq!(secure_getenv(&env, b"X", false), None);
    assert_eq!(secure_getenv(&env, b"A=B", false), None);
}
