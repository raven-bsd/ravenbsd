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
fn strtog_flags_match_gdtoa_h() {
    assert_eq!(STRTOG_ZERO, 0);
    assert_eq!(STRTOG_NORMAL, 1);
    assert_eq!(STRTOG_DENORMAL, 2);
    assert_eq!(STRTOG_INFINITE, 3);
    assert_eq!(STRTOG_NAN, 4);
    assert_eq!(STRTOG_NANBITS, 5);
    assert_eq!(STRTOG_NONUMBER, 6);
    assert_eq!(STRTOG_RETMASK, 7);
    assert_eq!(STRTOG_NEG, 0x08);
    assert_eq!(STRTOG_INEXLO, 0x10);
    assert_eq!(STRTOG_INEXHI, 0x20);
    assert_eq!(STRTOG_INEXACT, 0x30);
    assert_eq!(STRTOG_UNDERFLOW, 0x40);
    assert_eq!(STRTOG_OVERFLOW, 0x80);
    assert_eq!(STRTOG_NOMEMORY, 0x100);
}

#[test]
fn fpi_rounding_is_flt_rounds_numbering() {
    assert_eq!(FPI_ROUND_ZERO, 0);
    assert_eq!(FPI_ROUND_NEAR, 1);
    assert_eq!(FPI_ROUND_UP, 2);
    assert_eq!(FPI_ROUND_DOWN, 3);
    assert_eq!(Fpi::double().sudden_underflow, 0);
}
