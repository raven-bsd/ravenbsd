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

//! `labs` boundary tests (the `llabs`/`imaxabs` aliases are covered in `mod_ut.rs`).

use super::labs;

#[test]
fn basic() {
    assert_eq!(labs(0), 0);
    assert_eq!(labs(-1), 1);
    assert_eq!(labs(1), 1);
}

#[test]
fn boundaries() {
    assert_eq!(labs(i64::MAX), i64::MAX);
    assert_eq!(labs(i64::MIN), i64::MIN);
    assert_eq!(labs(i64::MIN + 1), i64::MAX);
}
