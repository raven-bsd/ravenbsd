// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::*;

#[test]
fn ulp_of_one_and_neighbours() {
    assert_eq!(ulp(1.0), f64::from_bits(0x3cb0_0000_0000_0000));
    assert_eq!(ulp(2.0), f64::from_bits(0x3cc0_0000_0000_0000));
    assert_eq!(ulp(-1.0), ulp(1.0));
    assert_eq!(ulp(0.0), f64::from_bits(0x0000_0000_0000_0001));
    assert_eq!(ulp(f64::from_bits(1)), f64::from_bits(1));
    assert_eq!(ulp(f64::MAX), f64::from_bits(0x7ca0_0000_0000_0000));
}
