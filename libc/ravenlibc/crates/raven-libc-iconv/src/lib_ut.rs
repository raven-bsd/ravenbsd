// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

//! Crate-level table budget tests.

use crate::tables_8bit::{CHARMAPS, LEGACY_CHARS};

#[test]
fn ring_one_tables_fit_the_budget() {
    let bytes = core::mem::size_of_val(CHARMAPS) + core::mem::size_of_val(LEGACY_CHARS);
    assert!(bytes <= 24 * 1024, "8-bit tables {bytes} bytes");
}
