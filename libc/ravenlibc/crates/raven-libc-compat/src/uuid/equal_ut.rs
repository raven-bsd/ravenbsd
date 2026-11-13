// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `uuid_equal` tests.

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
fn equal_on_identical_values() {
    let uuid = Uuid {
        time_low: 0x1234,
        node: [1, 2, 3, 4, 5, 6],
        ..Uuid::default()
    };
    assert!(equal(&uuid, &uuid));
    assert!(equal(&uuid, &uuid.clone()));
}

#[test]
fn unequal_on_one_field() {
    let a = Uuid::default();
    let b = Uuid {
        clock_seq_low: 1,
        ..Uuid::default()
    };
    assert!(!equal(&a, &b));
}
