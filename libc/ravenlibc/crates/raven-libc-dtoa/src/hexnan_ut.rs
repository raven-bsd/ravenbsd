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
use crate::types::Fpi;

#[test]
fn nan_payload_is_parsed() {
    let fpi = Fpi {
        nbits: 52,
        emin: 1 - 1023 - 53 + 1,
        emax: 2046 - 1023 - 53 + 1,
        rounding: FPI_ROUND_NEAR,
        sudden_underflow: 0,
    };
    let mut bits = [0u32; 2];
    let mut pos = 3usize; // at '(' of "nan(0x1)"
    let k = hexnan(b"nan(0x1)", &mut pos, &fpi, &mut bits);
    assert_eq!(k, STRTOG_NANBITS);
    assert_eq!(pos, 8);
    assert_ne!(bits[0] | bits[1], 0);
}

#[test]
fn malformed_payload_is_consumed_and_ignored() {
    let fpi = Fpi {
        nbits: 52,
        emin: 1 - 1023 - 53 + 1,
        emax: 2046 - 1023 - 53 + 1,
        rounding: FPI_ROUND_NEAR,
        sudden_underflow: 0,
    };
    let mut bits = [0u32; 2];
    let mut pos = 3usize;
    let k = hexnan(b"nan(abc)", &mut pos, &fpi, &mut bits);
    assert_eq!(k, STRTOG_NANBITS);
    assert_eq!(pos, 8);
}
