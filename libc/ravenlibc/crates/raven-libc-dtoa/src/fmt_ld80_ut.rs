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

const ONE_POINT_FIVE: [u8; 16] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xc0, 0xff, 0x3f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

#[test]
fn strtopx_round_trips_ld80() {
    let mut ctx = DtoaCtx::new();
    let (v, n, k) = strtorx(&mut ctx, b"1.5", &C_NUMERIC, Rounding::Nearest).unwrap();
    assert_eq!(n, 3);
    assert_eq!(k & STRTOG_RETMASK, STRTOG_NORMAL);
    assert_eq!(v.0, ONE_POINT_FIVE);
}

#[test]
fn ultox_special_patterns() {
    let inf = ultox(&[0, 0], 0, STRTOG_INFINITE);
    assert_eq!(&inf.0[..10], &[0, 0, 0, 0, 0, 0, 0, 0x80, 0xff, 0x7f]);
    let nan = ultox(&[0, 0], 0, STRTOG_NAN);
    assert_eq!(&nan.0[..10], &[0, 0, 0, 0, 0, 0, 0, 0xc0, 0xff, 0x7f]);
    let neg = ultox(&[0, 0xc000_0000], -63, STRTOG_NORMAL | STRTOG_NEG);
    assert_eq!(neg.0[9], 0xbf);
}
