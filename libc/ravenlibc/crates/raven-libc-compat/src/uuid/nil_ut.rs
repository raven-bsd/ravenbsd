// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `uuid_is_nil` tests.

#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::byte_char_slices,
    clippy::field_reassign_with_default,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

use super::*;
use crate::uuid::Uuid;

#[test]
fn default_is_nil() {
    assert!(is_nil(&Uuid::default()));
}

#[test]

fn one_nonzero_field_is_not_nil() {
    let mut uuid = Uuid::default();
    uuid.node[3] = 1;
    assert!(!is_nil(&uuid));
    let mut uuid = Uuid::default();
    uuid.time_low = 1;
    assert!(!is_nil(&uuid));
}
