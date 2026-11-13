// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `uuid_hash` tests.

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
fn hash_is_the_low_sixteen_bits_of_time_low() {
    let uuid = Uuid {
        time_low: 0x1234_abcd,
        ..Uuid::default()
    };

    assert_eq!(hash(&uuid), 0xabcd);
}

#[test]
fn hash_of_nil_is_zero() {
    assert_eq!(hash(&Uuid::default()), 0);
}
