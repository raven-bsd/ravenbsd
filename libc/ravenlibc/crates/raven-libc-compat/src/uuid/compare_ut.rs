// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `uuid_compare` tests: the donor's field order.

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

fn base() -> Uuid {
    Uuid {
        time_low: 1,
        time_mid: 2,
        time_hi_and_version: 3,
        clock_seq_hi_and_reserved: 4,

        clock_seq_low: 5,
        node: [6, 7, 8, 9, 10, 11],
    }
}

#[test]
fn equal_values_compare_zero() {
    assert_eq!(compare(&base(), &base()), 0);
}

#[test]
fn every_field_decides_in_order() {
    let mut other = base();
    other.time_low = 0;
    assert_eq!(compare(&other, &base()), -1);
    let mut other = base();
    other.time_mid = 3;
    assert_eq!(compare(&other, &base()), 1);
    let mut other = base();
    other.time_hi_and_version = 2;
    assert_eq!(compare(&other, &base()), -1);
    let mut other = base();
    other.clock_seq_hi_and_reserved = 3;
    assert_eq!(compare(&other, &base()), -1);
    let mut other = base();
    other.clock_seq_low = 6;
    assert_eq!(compare(&other, &base()), 1);
    let mut other = base();
    other.node[5] = 12;
    assert_eq!(compare(&other, &base()), 1);
}

#[test]
fn unsigned_field_order() {
    let high = Uuid {
        time_low: 0xffff_ffff,
        ..Uuid::default()
    };
    let low = Uuid {
        time_low: 1,
        ..Uuid::default()
    };
    assert_eq!(compare(&high, &low), 1);
}
