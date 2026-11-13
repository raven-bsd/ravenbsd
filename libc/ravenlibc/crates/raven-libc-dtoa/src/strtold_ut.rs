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
use crate::types::{STRTOG_NORMAL, STRTOG_RETMASK};
use crate::{C_NUMERIC, Rounding};

#[cfg(target_arch = "x86_64")]
#[test]
fn strtold_ld80_known() {
    let mut ctx = DtoaCtx::new();
    let cases: &[(&[u8], [u8; 16], usize)] = &[
        (
            b"0x1.234p0",
            [
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xa0, 0x91, 0xff, 0x3f, 0, 0, 0, 0, 0, 0,
            ],
            9,
        ),
        (
            b"1.5",
            [
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xc0, 0xff, 0x3f, 0, 0, 0, 0, 0, 0,
            ],
            3,
        ),
    ];
    for (text, want, consumed) in cases {
        let (v, n, k) = strtold(&mut ctx, text, &C_NUMERIC, Rounding::Nearest).unwrap();
        assert_eq!(n, *consumed, "consumed for {text:?}");
        assert_eq!(v.0, *want, "bytes for {text:?}");
        assert_eq!(k & STRTOG_RETMASK, STRTOG_NORMAL);
    }
    let (v, _, _) = strtold(&mut ctx, b"-0x0", &C_NUMERIC, Rounding::Nearest).unwrap();
    assert_eq!(v.0[9] & 0x80, 0x80, "negative zero");
}

#[test]
fn two_digit_integer_normalises_once() {
    // Regression: the increment path computed `KMASK - (ULbits - (rvbits & KMASK))` instead of
    // `KMASK & (...)`, so a 64-bit significand grew to 65 bits and came back shifted by one.
    let mut ctx = DtoaCtx::new();
    let (v, n, _) = strtold(&mut ctx, b"38e026", &C_NUMERIC, Rounding::Nearest).unwrap();
    assert_eq!(n, 6);
    assert_eq!(
        v.0,
        [
            0xde, 0x74, 0x1e, 0x5b, 0xcc, 0x94, 0x74, 0xc4, 0x5a, 0x40, 0, 0, 0, 0, 0, 0
        ]
    );
}
