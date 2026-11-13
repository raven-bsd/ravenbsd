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

#[test]
fn ftoa_shortest_round_trips() {
    let mut ctx = DtoaCtx::new();
    let mut out = [0u8; 128];
    let cases: &[(f32, &[u8], i32)] = &[
        (1.5, b"15", 1),
        (0.1, b"1", 0),
        (1.0, b"1", 1),
        (f32::MAX, b"34028235", 39),
        (f32::MIN_POSITIVE, b"11754944", -37),
    ];
    for (value, want, decpt) in cases {
        let d = ftoa(&mut ctx, *value, 0, 0, Rounding::Nearest, &mut out).unwrap();
        assert_eq!(&out[..d.len], *want, "digits for {value}");
        assert_eq!(d.decpt, *decpt, "decpt for {value}");
        assert_eq!(d.kind, Kind::Finite);
    }
}

#[test]
fn ftoa_special_values() {
    let mut ctx = DtoaCtx::new();
    let mut out = [0u8; 128];
    let z = ftoa(&mut ctx, -0.0, 0, 0, Rounding::Nearest, &mut out).unwrap();
    assert_eq!(z.kind, Kind::Zero);
    assert!(z.negative);
    let inf = ftoa(&mut ctx, f32::INFINITY, 0, 0, Rounding::Nearest, &mut out).unwrap();
    assert_eq!(inf.kind, Kind::Inf);
    let nan = ftoa(&mut ctx, f32::NAN, 0, 0, Rounding::Nearest, &mut out).unwrap();
    assert_eq!(nan.kind, Kind::Nan);
}
