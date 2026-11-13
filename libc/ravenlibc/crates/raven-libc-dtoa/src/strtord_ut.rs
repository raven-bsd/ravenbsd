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
use crate::C_NUMERIC;
use crate::ctx::DtoaCtx;

#[test]
fn strtord_double_values() {
    let mut ctx = DtoaCtx::new();
    let (v, n, k) = strtord(&mut ctx, b"1.5", &C_NUMERIC, Rounding::Nearest).unwrap();
    assert_eq!(v, 1.5);
    assert_eq!(n, 3);
    assert_eq!(k & STRTOG_RETMASK, STRTOG_NORMAL);
}

#[test]
fn ultod_packs_all_kinds() {
    let normal = ultod(&[0, 0x0018_0000], -52, STRTOG_NORMAL);
    assert_eq!(normal, 1.5);
    let zero = ultod(&[0, 0], 0, STRTOG_ZERO);
    assert_eq!(zero.to_bits(), 0);
    let inf = ultod(&[0, 0], 0, STRTOG_INFINITE);
    assert!(inf.is_infinite());
    let nan = ultod(&[0, 0], 0, STRTOG_NAN);
    assert!(nan.is_nan());
    let neg = ultod(&[0, 0x0018_0000], -52, STRTOG_NORMAL | STRTOG_NEG);
    assert_eq!(neg, -1.5);
}
