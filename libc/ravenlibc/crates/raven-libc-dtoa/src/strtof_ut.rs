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
use crate::ctx::DtoaCtx;
use crate::{C_NUMERIC, Rounding};

fn parse(text: &[u8]) -> (f32, usize, i32) {
    let mut ctx = DtoaCtx::new();
    strtof(&mut ctx, text, &C_NUMERIC, Rounding::Nearest).unwrap()
}

#[test]
fn float_values_are_not_double_rounded() {
    assert_eq!(parse(b"1.5").0, 1.5);
    assert_eq!(parse(b"0.1").0.to_bits(), 0x3dcc_cccd);
    assert_eq!(parse(b"3.4028235e38").0, f32::MAX);
    assert_eq!(parse(b"1.1754944e-38").0, f32::MIN_POSITIVE);
    assert_eq!(parse(b"1e-45").0.to_bits(), 1);
    assert_eq!(parse(b"inf").0, f32::INFINITY);
    assert!(parse(b"nan").0.is_nan());
    assert_eq!(parse(b"8388608.5000000001").0, 8_388_609.0);
}

#[test]
fn negative_zero_and_overflow_modes() {
    let (v, _, _) = parse(b"-0.0");
    assert_eq!(v.to_bits(), 0x8000_0000);
    let mut ctx = DtoaCtx::new();
    let (v, _, flags) = strtof(&mut ctx, b"1e50", &C_NUMERIC, Rounding::Nearest).unwrap();
    assert!(v.is_infinite());
    assert!(flags & STRTOG_OVERFLOW != 0);
}
