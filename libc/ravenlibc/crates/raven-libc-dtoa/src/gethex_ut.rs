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

fn parse_hex(text: &[u8]) -> (f64, usize, i32) {
    let mut ctx = DtoaCtx::new();
    let fpi = Fpi::double();
    let mut bits = [0u32; 2];
    let mut expt = 0i32;
    let mut b = BigRef::NIL;
    let mut s = 0usize;
    let k = gethex(
        &mut ctx, text, &mut s, &fpi, &mut expt, &mut b, 0, &C_NUMERIC,
    )
    .unwrap();
    if !b.is_nil() {
        crate::bigint::copybits(&mut bits, fpi.nbits, b, &ctx);
    }
    (crate::strtord::ultod(&bits, expt, k), s, k)
}

#[test]
fn hex_forms() {
    let (v, n, k) = parse_hex(b"0x1.8p3");
    assert_eq!(v, 12.0);
    assert_eq!(n, 7);
    assert_eq!(k & STRTOG_RETMASK, STRTOG_NORMAL);
    assert_eq!(parse_hex(b"0x1p0").0, 1.0);
    assert_eq!(parse_hex(b"0x.8p1").0, 1.0);
    assert_eq!(parse_hex(b"0x1p-1074").0.to_bits(), 1);
    assert_eq!(parse_hex(b"0x1p-1075").0, 0.0);
    let (v, _, _) = parse_hex(b"0x1.fffffffffffffp1023");
    assert_eq!(v, f64::MAX);
    let (_, n, k) = parse_hex(b"0x");
    assert_eq!(n, 1);
    assert_eq!(k & STRTOG_RETMASK, STRTOG_ZERO);
    let separator = NumericLocale {
        decimal_point: b",",
    };
    let mut ctx = DtoaCtx::new();
    let fpi = Fpi::double();
    let mut bits = [0u32; 2];
    let mut expt = 0i32;
    let mut b = BigRef::NIL;
    let mut s = 0usize;
    let k = gethex(
        &mut ctx, b"0x1,8p0", &mut s, &fpi, &mut expt, &mut b, 0, &separator,
    )
    .unwrap();
    crate::bigint::copybits(&mut bits, fpi.nbits, b, &ctx);
    assert_eq!(crate::strtord::ultod(&bits, expt, k), 1.5);
    assert_eq!(s, 7);
}

#[test]
fn huge_exponents_saturate_by_rounding() {
    let mut ctx = DtoaCtx::new();
    let fpi = Fpi::double();
    let mut expt = 0i32;
    let mut b = BigRef::NIL;
    let mut s = 0usize;
    let k = gethex(
        &mut ctx,
        b"0x1p999999999",
        &mut s,
        &fpi,
        &mut expt,
        &mut b,
        0,
        &C_NUMERIC,
    )
    .unwrap();
    assert!(k & STRTOG_OVERFLOW != 0);
    assert!(b.is_nil());
}
