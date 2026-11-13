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

fn parse_double(text: &[u8]) -> (f64, usize, i32) {
    let mut ctx = DtoaCtx::new();
    let fpi = Fpi::double();
    let mut bits = [0u32; 2];
    let mut expt = 0i32;
    let (n, k) = strtodg(&mut ctx, text, &fpi, &mut expt, &mut bits, &C_NUMERIC).unwrap();
    (crate::strtord::ultod(&bits, expt, k), n, k)
}

#[test]
fn generic_double_parser() {
    let (v, n, k) = parse_double(b"1.5");
    assert_eq!(v, 1.5);
    assert_eq!(n, 3);
    assert_eq!(k & STRTOG_RETMASK, STRTOG_NORMAL);
    assert_eq!(parse_double(b"0").0, 0.0);
    assert_eq!(parse_double(b"inf").0, f64::INFINITY);
    assert!(parse_double(b"nan").0.is_nan());
    assert_eq!(parse_double(b"0x1p-1074").0.to_bits(), 1);
}

#[test]
fn rounding_directions() {
    let mut ctx = DtoaCtx::new();
    let fpi = Fpi::double().with_rounding(FPI_ROUND_UP);
    let mut bits = [0u32; 2];
    let mut expt = 0i32;
    let (_, kk) = strtodg(&mut ctx, b"0.1", &fpi, &mut expt, &mut bits, &C_NUMERIC).unwrap();
    let _ = kk;
    let up = crate::strtord::ultod(&bits, expt, STRTOG_NORMAL);
    let mut ctx = DtoaCtx::new();
    let fpi = Fpi::double().with_rounding(FPI_ROUND_DOWN);
    strtodg(&mut ctx, b"0.1", &fpi, &mut expt, &mut bits, &C_NUMERIC).unwrap();
    let down = crate::strtord::ultod(&bits, expt, STRTOG_NORMAL);
    assert!(up > down);
    let _ = Rounding::Nearest;
}

#[test]
fn long_leading_zero_run_skips_no_digit() {
    // Regression: `s2b` was told to skip a separator byte although `s0` already points at the
    // first significant fractional digit.
    let mut text = b"0.".to_vec();
    text.extend(std::iter::repeat_n(b'0', 20));
    text.extend_from_slice(b"1490969259563407275");
    let mut ctx = DtoaCtx::new();
    let fpi = Fpi::ld80();
    let mut bits = [0u32; 2];
    let mut expt = 0i32;
    let (n, k) = strtodg(&mut ctx, &text, &fpi, &mut expt, &mut bits, &C_NUMERIC).unwrap();
    assert_eq!(n, text.len());
    assert_eq!(k & STRTOG_RETMASK, STRTOG_NORMAL);
    assert_eq!(expt, -133);
    assert_eq!(bits, [0x9d62_7bb1, 0xe14f_1459]);
}
