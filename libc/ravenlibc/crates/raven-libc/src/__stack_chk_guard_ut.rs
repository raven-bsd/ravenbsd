// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

//! The shell guard storage: a word-wide store and read.

use super::*;

#[test]
fn the_guard_round_trips_a_word() {
    set(0x5a5a_5a5a_5a5a_5a5a);
    assert_eq!(get(), 0x5a5a_5a5a_5a5a_5a5a);
    set(0);
    assert_eq!(get(), 0);
}
